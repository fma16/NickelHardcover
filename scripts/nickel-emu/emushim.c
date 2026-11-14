// Test-only LD_PRELOAD shim for running genuine Kobo Nickel under qemu-arm.
// It is NOT part of the plugin. It fakes the pieces of Kobo hardware that Nickel
// cannot start without:
//   - /dev/fb0: a plain file (EMU_FB, default /emu/fb.raw) plus the fbdev ioctls;
//     e-ink update ioctls (hwtcon) succeed and do nothing.
//   - /dev/input/eventN: FIFOs (symlinks to /emu/input/eventN) that a test script fills
//     with struct input_event records; the evdev ioctls describe a multitouch type B
//     panel on event1 and a key device on the others.
// Every /dev, /sys and /proc open and every unknown ioctl is logged to EMU_SHIM_LOG
// so missing hardware can be identified.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <linux/fb.h>
#include <linux/input.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

static int (*real_open)(const char *, int, ...);
static int (*real_open64)(const char *, int, ...);
static int (*real_ioctl)(int, unsigned long, ...);
static int (*real_close)(int);

static int fb_fd = -1;
static int ntx_fd = -1;  // /dev/ntx_io: board controller (frontlight, LEDs); every request succeeds
static int log_fd = -1;
static struct fb_var_screeninfo vinfo;
static int vinfo_ready;

static int env_int(const char *name, int fallback) {
	const char *v = getenv(name);
	return v && *v ? atoi(v) : fallback;
}

static void init(void) {
	if (real_open) return;
	real_open = dlsym(RTLD_NEXT, "open");
	real_open64 = dlsym(RTLD_NEXT, "open64");
	real_ioctl = dlsym(RTLD_NEXT, "ioctl");
	real_close = dlsym(RTLD_NEXT, "close");
	const char *path = getenv("EMU_SHIM_LOG");
	if (path && *path) log_fd = real_open(path, O_WRONLY | O_CREAT | O_APPEND, 0644);
}

static void logf_(const char *fmt, ...) {
	if (log_fd < 0) return;
	char buf[512];
	va_list ap;
	va_start(ap, fmt);
	int n = vsnprintf(buf, sizeof buf, fmt, ap);
	va_end(ap);
	if (n > (int)sizeof buf - 1) n = sizeof buf - 1;
	if (n > 0) (void)!write(log_fd, buf, n);
}

static void init_vinfo(void) {
	if (vinfo_ready) return;
	memset(&vinfo, 0, sizeof vinfo);
	vinfo.xres = vinfo.xres_virtual = env_int("EMU_FB_W", 1264);
	vinfo.yres = vinfo.yres_virtual = env_int("EMU_FB_H", 1680);
	vinfo.bits_per_pixel = env_int("EMU_FB_BPP", 32);
	if (vinfo.bits_per_pixel == 32) {
		vinfo.red.offset = 16; vinfo.red.length = 8;
		vinfo.green.offset = 8; vinfo.green.length = 8;
		vinfo.blue.offset = 0; vinfo.blue.length = 8;
		vinfo.transp.offset = 24; vinfo.transp.length = 8;
	} else if (vinfo.bits_per_pixel == 8) {
		vinfo.grayscale = 1;
		vinfo.red.length = vinfo.green.length = vinfo.blue.length = 8;
	}
	vinfo.rotate = env_int("EMU_FB_ROTATE", 0);
	vinfo.width = vinfo.xres * 254 / 3000;   // 300 dpi
	vinfo.height = vinfo.yres * 254 / 3000;
	vinfo_ready = 1;
}

static size_t fb_bytes(void) {
	unsigned w = vinfo.xres_virtual > vinfo.yres_virtual ? vinfo.xres_virtual : vinfo.yres_virtual;
	// Room for either orientation and a virtual area twice the screen.
	return (size_t)w * w * (vinfo.bits_per_pixel / 8) * 2;
}

static int open_fb(void) {
	init_vinfo();
	const char *path = getenv("EMU_FB");
	int fd = real_open(path && *path ? path : "/emu/fb.raw", O_RDWR | O_CREAT, 0644);
	if (fd >= 0) {
		(void)!ftruncate(fd, fb_bytes());
		fb_fd = fd;
	}
	logf_("fb: open -> fd %d (%ux%u %ubpp)\n", fd, vinfo.xres, vinfo.yres, vinfo.bits_per_pixel);
	return fd;
}

#define MAX_INPUT 8
static struct { int fd; int index; } inputs[MAX_INPUT];
static int n_inputs;

static int input_index(int fd) {
	for (int i = 0; i < n_inputs; i++)
		if (inputs[i].fd == fd) return inputs[i].index;
	return -1;
}

static void set_bit(unsigned char *bits, size_t len, unsigned bit) {
	if (bit / 8 < len) bits[bit / 8] |= 1 << (bit % 8);
}

static int input_ioctl(int index, unsigned long req, void *arg) {
	unsigned nr = _IOC_NR(req);
	size_t len = _IOC_SIZE(req);
	int touch = index == 1;
	logf_("input%d: ioctl nr=%#x size=%zu\n", index, nr, len);
	if (_IOC_DIR(req) == _IOC_READ && arg) memset(arg, 0, len);
	if (nr == _IOC_NR(EVIOCGVERSION)) { *(int *)arg = EV_VERSION; return 0; }
	if (nr == _IOC_NR(EVIOCGID)) return 0;
	if (nr == _IOC_NR(EVIOCGNAME(0))) {
		const char *name = touch ? "cyttsp5_mt" : "ntx_event0";
		strncpy(arg, name, len ? len - 1 : 0);
		return strlen(name);
	}
	if (nr == _IOC_NR(EVIOCGRAB)) return 0;
	if (nr >= 0x20 && nr < 0x40) {  // EVIOCGBIT(type)
		unsigned type = nr - 0x20;
		if (type == 0) {
			set_bit(arg, len, EV_SYN);
			set_bit(arg, len, EV_KEY);
			if (touch) set_bit(arg, len, EV_ABS);
		} else if (type == EV_ABS && touch) {
			set_bit(arg, len, ABS_MT_SLOT);
			set_bit(arg, len, ABS_MT_TOUCH_MAJOR);
			set_bit(arg, len, ABS_MT_POSITION_X);
			set_bit(arg, len, ABS_MT_POSITION_Y);
			set_bit(arg, len, ABS_MT_TRACKING_ID);
			set_bit(arg, len, ABS_MT_PRESSURE);
		} else if (type == EV_KEY) {
			if (touch) set_bit(arg, len, BTN_TOUCH);
			else { set_bit(arg, len, KEY_POWER); set_bit(arg, len, KEY_F23); set_bit(arg, len, KEY_F24); }
		}
		return len;
	}
	if (nr >= 0x40 && nr < 0x80 && touch) {  // EVIOCGABS(axis)
		struct input_absinfo *a = arg;
		switch (nr - 0x40) {
		case ABS_MT_POSITION_X: a->maximum = env_int("EMU_TOUCH_MAX_X", 1680); break;
		case ABS_MT_POSITION_Y: a->maximum = env_int("EMU_TOUCH_MAX_Y", 1264); break;
		case ABS_MT_SLOT: a->maximum = 9; break;
		case ABS_MT_TRACKING_ID: a->maximum = 65535; break;
		default: a->maximum = 255;
		}
		return 0;
	}
	return 0;
}

static int interesting(const char *p) {
	return !strncmp(p, "/dev/", 5) || !strncmp(p, "/sys/", 5) || !strncmp(p, "/proc/", 6);
}

static int do_open(int (*fn)(const char *, int, ...), const char *path, int flags, mode_t mode) {
	init();
	if (path && !strcmp(path, "/dev/fb0")) return open_fb();
	int input = path && !strncmp(path, "/dev/input/event", 16) ? atoi(path + 16) : -1;
	// Read-write keeps the FIFO from reporting EOF whenever the injecting script closes it.
	if (input >= 0) flags = (flags & ~O_ACCMODE) | O_RDWR | O_NONBLOCK;
	int fd = fn(path, flags, mode);
	if (input >= 0 && fd >= 0 && n_inputs < MAX_INPUT) {
		inputs[n_inputs].fd = fd;
		inputs[n_inputs++].index = input;
	}
	if (path && fd >= 0 && !strcmp(path, "/dev/ntx_io")) ntx_fd = fd;
	if (path && interesting(path) && strncmp(path, "/proc/self", 10) && strcmp(path, "/dev/null") &&
	    strcmp(path, "/dev/urandom")) {
		int e = errno;
		logf_("open %s flags=%#x -> %d%s%s\n", path, flags, fd, fd < 0 ? " " : "", fd < 0 ? strerror(e) : "");
		errno = e;
	}
	return fd;
}

int open(const char *path, int flags, ...) {
	mode_t mode = 0;
	if (flags & O_CREAT) { va_list ap; va_start(ap, flags); mode = va_arg(ap, mode_t); va_end(ap); }
	init();
	return do_open(real_open, path, flags, mode);
}

int open64(const char *path, int flags, ...) {
	mode_t mode = 0;
	if (flags & O_CREAT) { va_list ap; va_start(ap, flags); mode = va_arg(ap, mode_t); va_end(ap); }
	init();
	return do_open(real_open64 ? real_open64 : real_open, path, flags | O_LARGEFILE, mode);
}

int close(int fd) {
	init();
	if (fd >= 0 && fd == fb_fd) { fb_fd = -1; logf_("fb: close\n"); }
	if (fd >= 0 && fd == ntx_fd) ntx_fd = -1;
	for (int i = 0; i < n_inputs; i++)
		if (inputs[i].fd == fd) { inputs[i] = inputs[--n_inputs]; break; }
	return real_close(fd);
}

static int fb_ioctl(unsigned long req, void *arg) {
	switch (req) {
	case FBIOGET_VSCREENINFO:
		memcpy(arg, &vinfo, sizeof vinfo);
		return 0;
	case FBIOPUT_VSCREENINFO: {
		struct fb_var_screeninfo *v = arg;
		logf_("fb: PUT_VSCREENINFO %ux%u virt %ux%u bpp %u rotate %u gray %u\n", v->xres, v->yres,
		      v->xres_virtual, v->yres_virtual, v->bits_per_pixel, v->rotate, v->grayscale);
		// hwtcon keeps xres/yres as requested: Nickel draws upright 1264x1680 with rotate=1.
		if (v->xres && v->yres && (size_t)v->xres * v->yres * 4 <= fb_bytes()) {
			vinfo.xres = vinfo.xres_virtual = v->xres;
			vinfo.yres = vinfo.yres_virtual = v->yres;
		}
		vinfo.rotate = v->rotate;
		memcpy(arg, &vinfo, sizeof vinfo);
		return 0;
	}
	case FBIOGET_FSCREENINFO: {
		struct fb_fix_screeninfo f;
		memset(&f, 0, sizeof f);
		strcpy(f.id, "hwtcon");
		f.type = FB_TYPE_PACKED_PIXELS;
		f.visual = vinfo.bits_per_pixel == 8 ? FB_VISUAL_STATIC_PSEUDOCOLOR : FB_VISUAL_TRUECOLOR;
		f.line_length = vinfo.xres_virtual * (vinfo.bits_per_pixel / 8);
		f.smem_len = fb_bytes();
		memcpy(arg, &f, sizeof f);
		return 0;
	}
	default:
		// E-ink controller requests (hwtcon): report success. Requests that return
		// data get zeroes.
		logf_("fb: ioctl %#lx dir=%lu type=%#lx nr=%lu size=%lu\n", req, _IOC_DIR(req), _IOC_TYPE(req),
		      _IOC_NR(req), _IOC_SIZE(req));
		if ((_IOC_DIR(req) & _IOC_READ) && arg && _IOC_SIZE(req) && !(_IOC_DIR(req) & _IOC_WRITE))
			memset(arg, 0, _IOC_SIZE(req));
		return 0;
	}
}

int ioctl(int fd, unsigned long req, ...) {
	va_list ap;
	va_start(ap, req);
	void *arg = va_arg(ap, void *);
	va_end(ap);
	init();
	if (fd >= 0 && fd == fb_fd) return fb_ioctl(req, arg);
	if (fd >= 0 && input_index(fd) >= 0) return input_ioctl(input_index(fd), req, arg);
	if (fd >= 0 && fd == ntx_fd) {
		logf_("ntx_io: ioctl nr=%lu arg=%p\n", req, arg);
		return 0;
	}
	int r = real_ioctl(fd, req, arg);
	if (r < 0 && _IOC_TYPE(req) != 'T' && req != FIONREAD) {
		int e = errno;
		logf_("ioctl fd=%d %#lx type=%#lx nr=%lu -> %s\n", fd, req, _IOC_TYPE(req), _IOC_NR(req), strerror(e));
		errno = e;
	}
	return r;
}

// Emulator-only: there is no Wi-Fi radio, so Nickel always reports "offline".
// While /emu/online exists, callers inside the plugin (libhardcover.so) are told the
// internet is reachable; Nickel itself keeps seeing the real answer. The plugin
// resolves this symbol with dlsym(RTLD_DEFAULT), which finds this preloaded copy first.
int _ZN23WirelessWorkflowManager20isInternetAccessibleEv(void *self) {
	static int (*real)(void *);
	if (!real) real = dlsym(RTLD_NEXT, "_ZN23WirelessWorkflowManager20isInternetAccessibleEv");
	Dl_info info;
	if (access("/emu/online", F_OK) == 0 && dladdr(__builtin_return_address(0), &info) && info.dli_fname &&
	    strstr(info.dli_fname, "libhardcover"))
		return 1;
	return real ? real(self) : 0;
}
