# Bookwise Nickel — Hardcover and Readwise Books on Kobo

> **This repository is a fork of [StrayRose's NickelHardcover](https://codeberg.org/StrayRose/NickelHardcover).**
> It adds Readwise Books while preserving the original project's Hardcover features.

Keep using **Hardcover** and send your new reading sessions to **Readwise Books**,
directly from your Kobo's reading menu. You can sync both services together or use
either service on its own.

**Experimental release. Kobo firmware 5.x is not supported.**
Back up your data before installing. NickelMenu is not required.

## What you can do

- Keep Hardcover's reading progress, statuses, ratings/reviews, annotations and
  reading journal, along with its QR code/OAuth sign-in.
- Send the progress and duration of your new reading sessions to Readwise Books.
- Sync on demand, automatically while reading, or when closing a book, including
  when putting your Kobo to sleep.
- Read offline with auto-sync enabled: completed sessions are saved on your Kobo
  for later upload.
- Keep accounts and book links separate: a failure in one service does not undo
  a successful sync with the other.
- In Readwise-only mode, link a book to your Readwise Books library, change its
  status, give it a star rating and write a review.

**The Readwise integration uses Books, not Reader or the highlights API.**
The journal, annotations and “Paused” status remain available with Hardcover,
including in dual-sync mode; they are not sent to Readwise.

## Installing or updating

### Before you start

You need a compatible Kobo, a USB cable, a computer and an account for each service
you use (with access to Readwise Books if you use Readwise). Your Kobo needs Wi-Fi
to send data, but not to read offline.

The original project excludes the **Kobo Mini (2012)**. This fork has been tried
on firmware **4.46.23852** in Readwise-only mode. Dual sync and offline recovery
still need on-device validation; local tests and ARM builds do not guarantee
compatibility with every model.

You also need a **`KoboRoot.tgz` built from this fork**. Downloads from the original
NickelHardcover project **do not include Readwise**. If you do not have this archive,
have it built from this repository or follow the developer-oriented
[build instructions](AGENTS.md#compilation-pour-kobo). No public download link for
this fork is provided here.

### Installing over USB

1. Connect your Kobo to your computer and accept the USB connection on the device.
2. Show hidden files so you can see the `.kobo` and `.adds` folders.
   On macOS: **Cmd + Shift + .**; on Windows: enable **Hidden items** in File
   Explorer; on Linux: usually **Ctrl + H**.
3. Back up `.kobo/KoboReader.sqlite` and the `.adds/NickelHardcover/` folder, if it
   already exists, to your computer.
4. Copy **`KoboRoot.tgz` without extracting it** into your Kobo's `.kobo/` folder.
5. Safely eject your Kobo, unplug it and wait for it to restart.

To update, follow the same steps: **there is no need to uninstall**, and your
configuration is preserved. This fork uses the same folders as NickelHardcover
and replaces it; do not install both plugins at the same time.

## Connecting Hardcover and Readwise Books

The `provider` setting in `.adds/NickelHardcover/config.ini` determines which
services are used:

| Value | Behavior |
| --- | --- |
| `dual` | Hardcover **and** new Readwise Books sessions |
| `hardcover` | Hardcover only; the default |
| `readwise` | Readwise Books only |

Switching modes does not delete tokens or book links. Close and reopen your book
after each change.

### Keeping or connecting your Hardcover account

If NickelHardcover was already connected, **keep your existing settings**,
especially `access_token`, `refresh_token` and `token_expires_at`: you do not need
to sign in again.

Otherwise, once `provider = dual` or `provider = hardcover` is configured, open
the reading menu and choose **Sign in to Hardcover.app**. Scan the QR code (or
open the link and enter the code), then tap **Continue**. This sign-in flow remains
available even if Readwise is already connected.

### Adding Readwise Books and enabling dual sync

Readwise sign-in is configured over USB, **not yet through a QR code**.

1. On your computer, sign in to Readwise and open
   <https://readwise.io/access_token> to get your **personal token**.
2. Connect your Kobo over USB again. Open `.adds/NickelHardcover/config.ini` in a
   text editor, or create it if it does not exist. Do not name it `config.ini.txt`.
3. Add or replace the settings below without deleting other existing settings
   or creating duplicate entries. Replace `YOUR_PERSONAL_TOKEN` with your token:

   ```ini
   provider = dual
   readwise_token = YOUR_PERSONAL_TOKEN
   readwise_publish_to_feed = true
   auto_sync_default = false
   retry_on_network = true
   sync_on_close = 1
   sync_on_read = 20
   sync_on_schedule = -1
   ```

4. Safely eject and unplug your Kobo, then close and reopen your book to load
   these settings.

If you only want Readwise, use `provider = readwise` instead. A missing token only
prevents uploads to its own service. In **Settings**, signing out of the main
account signs out of Hardcover in dual-sync mode; the **Readwise Books** row lets
you sign out of Readwise separately.

**Privacy:** the token is stored in plain text in this file on your Kobo. Do not
share the file or your token in an issue, screenshot or chat. Sessions are
**published to the Readwise Books feed by default**. To keep them out of the feed,
change `readwise_publish_to_feed = true` to `false` before reading. They will still
count towards Readwise statistics and leaderboards. This change does not affect
sessions already waiting to be uploaded.

## Using the reading menu

Open a book, tap the screen to show the reading toolbar, then open the added menu
**at the top right**. The plugin's menu labels are in English.

### Sending your first session

1. Enable Wi-Fi on your Kobo.
2. Read until the progress percentage increases.
3. Without leaving the book, open the menu and choose **Sync now (both services)**
   when both accounts are configured, or **Sync now** in single-service mode.
4. Check the progress in Hardcover and the session on the correct book in Readwise
   Books, depending on which services you use. Hardcover requires at least 2%
   progress; Readwise can record a session from 0% to 1%.

A manual sync also sends saved Readwise sessions waiting to be uploaded for other
books. If you only sync manually, use **Sync now before closing the book or putting
your Kobo to sleep**: saving sessions automatically when you close a book requires
**Readwise** auto-sync to be enabled for that book. Enabling only Hardcover
auto-sync is not enough.

### Enabling automatic sync

In dual-sync mode, enable **Enable auto-sync (Hardcover)** and
**Enable auto-sync (Readwise)** for the open book. These settings are independent
and apply **per book**; the corresponding **Disable auto-sync** entries turn them
off. In single-service mode, the entry is called **Enable auto-sync**.

With the configuration above, uploads are triggered when closing the book or
putting the Kobo to sleep at a progress threshold of 1 percentage point, and while
reading at a threshold of 20 percentage points. Retries when Wi-Fi reconnects are
enabled; scheduled daily sync is disabled. The thresholds and schedule are shared,
but evaluated separately against the progress already synced with each service.

In **Settings**, you can change the thresholds, choose a daily sync time or enable
**Enable auto-sync by default for new books**. Without Wi-Fi, completed and saved
Readwise sessions remain queued, even after a restart; reconnect your Kobo and use
**Sync now** if needed. Hardcover's retry queue stays in memory, as in the original
project: it does not offer the same persistence across restarts.

### Linking the correct book

**In dual-sync mode, the book-linking menus apply to Hardcover.** Use
**Manually link book** to search for and select the Hardcover book or edition.
An existing Readwise link is still used and is never replaced by the Hardcover ID.

For Readwise, the plugin tries to identify the book by its ISBN/ASIN, falling back
to its title and author. If the result is not right:

1. Make sure the book you want is already in your Readwise Books library.
   In dual-sync mode, temporarily switch to `provider = readwise` over USB, then
   close and reopen your book to access Readwise search.
2. Choose **Manually link book**, search for the book and select it. Search covers
   your library, not the entire Readwise catalog.
3. If you changed modes, restore `provider = dual` over USB. Close and reopen the
   book on your Kobo to apply the link and mode.

If a link already exists, use **Unlink book** before choosing another one. Without
an ISBN, an initial sync using the title and author may be needed before you can
rate the book.

Sessions that have already been saved keep their existing link. To correct a
session that has not been sent, follow the
[advanced procedure](AGENTS.md#recuperation-des-sessions-readwise) rather than
resending it without checking.

### Updating a status or writing a review

- **Update book status**: choose **Want to Read**, **Currently Reading**, **Read**,
  **Did Not Finish**, or **Paused** with Hardcover.
- **Write a review**: give the book a star rating and write your review.

In dual-sync mode, these actions apply to **Hardcover only**, as do
**Open reading journal** and **All annotations**. Reviews and statuses entered
here are not copied to Readwise. Hardcover journal options and annotation sync
remain available in **Settings**.

Hardcover keeps its rounding from 99% to 100% and disables its auto-sync when the
book is finished. Readwise keeps 99% without rounding, and its auto-sync remains
independent. To mark a book as finished there manually, use Readwise Books or the
**Read** menu option in Readwise-only mode.

## Frequently asked questions and troubleshooting

**Why am I being offered Hardcover sign-in?**

Hardcover is still the default mode, and its sign-in is also offered in dual-sync
mode if it is missing. To use both services, keep `provider = dual` and configure
both accounts. Choose `provider = readwise` only if you want Readwise alone, then
close and reopen the book.

**Why is no session appearing?**

Check Wi-Fi, your token and your account's access to Readwise Books. A session is
only saved if the progress percentage increased: a few pages may not be enough.
When syncing manually, sync before leaving the book; when syncing automatically,
check that **Readwise** auto-sync is enabled for that book. Dual-sync mode sends
new sessions: it does not reconstruct your history from Hardcover.

**Does the duration exactly match my reading time?**

It measures time spent in the reading view, excluding sleep and other screens.
Leaving a book open without reading or skipping pages can distort the measurement.
Completed sessions are saved, but an abrupt shutdown can lose the active session.

**A message asks me to check a pending session. What should I do?**

After a network interruption, the plugin may not know whether Readwise received
the session. It then blocks new **Readwise** uploads, without blocking Hardcover,
to prevent duplicates. **Do not delete the queue or blindly force a resend.**
Check Readwise first, back up `.adds/NickelHardcover/`, then see the
[advanced recovery procedure](AGENTS.md#recuperation-des-sessions-readwise).

**Can I switch Readwise accounts?**

Not by simply replacing the token: sessions from the old account could be sent to
the new one. Follow the [account-switching procedure](AGENTS.md#changer-de-compte-readwise).

**Can I go back to using just one service?**

Yes. Set `provider = hardcover` or `provider = readwise` in `config.ini`, then close
and reopen the book. The other service's tokens and book links are kept, but it
stops syncing. Any pending Readwise sessions remain on your Kobo; they are neither
deleted nor transferred to Hardcover.

## Uninstalling

1. Sync any sessions you want to keep and back up `.adds/NickelHardcover/`.
2. Delete that folder from your Kobo over USB.
3. Safely eject your Kobo and restart it manually.

## Credits and development

Thanks to StrayRose for [NickelHardcover](https://codeberg.org/StrayRose/NickelHardcover)
and to pgaskin for [NickelHook](https://github.com/pgaskin/NickelHook) and
[NickelTC](https://github.com/pgaskin/NickelTC). The project is distributed under the
[MIT license](LICENSE), with the original copyright preserved.

The images in `screenshots/` come from the original Hardcover project: they do not
show the Readwise integration. Build instructions, tests, architecture and technical
precautions are documented in [AGENTS.md](AGENTS.md).

## TODO

- [ ] Add Readwise Books sign-in via a QR code on the Kobo: scan it with a phone,
  sign in and authorize access through Readwise, then finish signing in on the
  e-reader without copying a token over USB.
  - Check [Readwise's](https://readwise.io/books_api) OAuth capabilities: Device
    Authorization Flow (unconfirmed), redirects, PKCE and token refresh.
  - Prefer a native Device Flow if available; otherwise, evaluate a temporary
    HTTPS pairing service and its hosting and security requirements.
  - Handle OAuth tokens with `Authorization: Bearer …`, expiration and
    authorization errors; keep personal-token configuration available.
  - Validate the complete flow on a real Kobo and phone.
