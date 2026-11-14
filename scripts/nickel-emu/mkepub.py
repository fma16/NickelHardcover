#!/usr/bin/env python3
"""Writes a small multi-chapter EPUB 2 with filler text. usage: mkepub.py OUT TITLE AUTHOR [ISBN]"""
import sys, zipfile
out, title, author = sys.argv[1:4]
isbn = sys.argv[4] if len(sys.argv) > 4 else None
words = ("letter glacier voyage creature night candle mountain winter science stranger journey lake "
         "storm silence memory harbour promise garden shadow morning river study wonder distance").split()
def para(seed):
    return ' '.join(words[(seed * 7 + i * 5) % len(words)] for i in range(90)).capitalize() + '.'
chapters = 12
with zipfile.ZipFile(out, 'w') as z:
    z.writestr('mimetype', 'application/epub+zip', zipfile.ZIP_STORED)
    z.writestr('META-INF/container.xml', '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>', zipfile.ZIP_DEFLATED)
    ident = f'<dc:identifier id="id" opf:scheme="ISBN">{isbn}</dc:identifier>' if isbn else '<dc:identifier id="id">bookwise-emu-test</dc:identifier>'
    items = ''.join(f'<item id="c{i}" href="c{i}.xhtml" media-type="application/xhtml+xml"/>' for i in range(chapters))
    refs = ''.join(f'<itemref idref="c{i}"/>' for i in range(chapters))
    z.writestr('content.opf', f'<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf"><dc:title>{title}</dc:title><dc:creator opf:role="aut">{author}</dc:creator><dc:language>en</dc:language>{ident}</metadata><manifest><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>{items}</manifest><spine toc="ncx">{refs}</spine></package>', zipfile.ZIP_DEFLATED)
    nav = ''.join(f'<navPoint id="n{i}" playOrder="{i + 1}"><navLabel><text>Chapter {i + 1}</text></navLabel><content src="c{i}.xhtml"/></navPoint>' for i in range(chapters))
    z.writestr('toc.ncx', f'<?xml version="1.0" encoding="utf-8"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head><meta name="dtb:uid" content="{isbn or "bookwise-emu-test"}"/></head><docTitle><text>{title}</text></docTitle><navMap>{nav}</navMap></ncx>', zipfile.ZIP_DEFLATED)
    for i in range(chapters):
        body = ''.join(f'<p>{para(i * 31 + p)}</p>' for p in range(14))
        z.writestr(f'c{i}.xhtml', f'<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter {i + 1}</title></head><body><h1>Chapter {i + 1}</h1>{body}</body></html>', zipfile.ZIP_DEFLATED)
