"""Recover selected, hash-pinned originals through their public download form."""
import hashlib
from html.parser import HTMLParser
from http.cookiejar import CookieJar
from pathlib import Path
import time
from urllib.parse import urlencode, urljoin, urlparse
from urllib.request import HTTPCookieProcessor, Request, build_opener


class DownloadForm(HTMLParser):
    def __init__(self, form_id):
        super().__init__()
        self.form_id=form_id
        self.active=False
        self.action=None
        self.fields={}

    def handle_starttag(self, tag, attrs):
        attrs=dict(attrs)
        if tag=='form' and attrs.get('id')==self.form_id:
            self.active=True
            self.action=attrs.get('action')
        elif self.active and tag=='input' and attrs.get('name'):
            self.fields[attrs['name']]=attrs.get('value','')

    def handle_endtag(self, tag):
        if tag=='form':self.active=False


def ensure_master(row, cache, download=False):
    target=Path(cache)/row['input']
    if target.is_file():
        raw=target.read_bytes()
    else:
        if not download:
            raise ValueError(f"Missing selected original {target}; rerun with --download to recover {row['source']}")
        if urlparse(row['source']).hostname!='lasonotheque.org' or not str(row['id']).isdigit():
            raise ValueError('Original source has no supported public recovery route')
        opener=build_opener(HTTPCookieProcessor(CookieJar()))
        headers={'User-Agent':'BlocklandReImagined content recovery','Referer':row['source']}
        request=Request('https://lasonotheque.org/download.php',
                        data=urlencode({'id':row['id'],'format':'wav'}).encode(),headers=headers)
        with opener.open(request,timeout=45) as response:
            page=response.read(256*1024+1)
        if len(page)>256*1024:
            raise ValueError('Original download form exceeds bounded page size')
        form=DownloadForm('dl-form');form.feed(page.decode('utf-8'))
        allowed={'id','format','antibot_hp','antibot_ctx','antibot_n','antibot_ts'}
        if (form.action!='/modules/telecharger.php' or not set(form.fields)<=allowed
                or form.fields.get('id')!=str(row['id']) or form.fields.get('format')!='wav'):
            raise ValueError('Public original download form changed; preserve cache and inspect author page')
        # Follow the published five-second public download countdown. No login,
        # CAPTCHA solving or alternative-format fallback is attempted here.
        time.sleep(5)
        request=Request(urljoin(row['source'],form.action),data=urlencode(form.fields).encode(),
                        headers=dict(headers,Referer='https://lasonotheque.org/download.php'))
        with opener.open(request,timeout=45) as response:
            raw=response.read(row['bytes']+1)
        if len(raw)!=row['bytes'] or hashlib.sha256(raw).hexdigest()!=row['input_sha256']:
            raise ValueError('Downloaded original differs from approved pinned bytes; no source adopted')
        target.parent.mkdir(parents=True,exist_ok=True)
        target.write_bytes(raw)
    if hashlib.sha256(raw).hexdigest()!=row['input_sha256']:
        raise ValueError(f"{row['key']}: cached original differs from pinned recording; not overwritten")
    return target
