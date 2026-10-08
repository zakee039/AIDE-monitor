"""Inspect or enable this repository's GitHub Pages using the existing Git login."""
import json, runpy, sys, urllib.request, urllib.error
from pathlib import Path
auth = runpy.run_path(str(Path(__file__).with_name('publish-release.py')))['token']()
def request(path, method='GET', body=None):
    req=urllib.request.Request('https://api.github.com/repos/zakee039/AIDE-monitor'+path,
        data=json.dumps(body).encode() if body is not None else None, method=method,
        headers={'Authorization':'Bearer '+auth,'Accept':'application/vnd.github+json','User-Agent':'AIDE-site-publisher','Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(req,timeout=45) as r:return json.load(r)
    except urllib.error.HTTPError as e:
        if e.code==404:return {'not_found':True}
        raise RuntimeError('GitHub request failed: '+str(e.code)) from None
pages=request('/pages')
if '--enable' in sys.argv:
    if pages.get('not_found'):
        pages=request('/pages','POST',{'build_type':'legacy','source':{'branch':'main','path':'/docs'}})
    elif pages.get('source') != {'branch':'main','path':'/docs'}:
        raise RuntimeError('Pages already has another source; preserve existing configuration.')
release=request('/releases/latest')
print(json.dumps({'pages':pages,'latest_release':{'tag':release.get('tag_name'),'url':release.get('html_url')}},ensure_ascii=True))
