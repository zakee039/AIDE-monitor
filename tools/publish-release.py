"""Publish an already-built, committed and tagged release using the existing Git login."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import urllib.error
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
REPO = "zakee039/AIDE-monitor"
VERSION = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))["version"]
TAG = "v" + VERSION
OUTPUT = ROOT / "artifacts" / ("release-" + TAG)
PREFIX = f"AIDE-monitor-{VERSION}-windows-x64"
FILES = [OUTPUT / (PREFIX + suffix) for suffix in [".exe", "-setup.exe", ".zip"]]
FILES.append(OUTPUT / "SHA256SUMS.txt")


def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=True).stdout.strip()


def token():
    value = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    if value:
        return value
    result = subprocess.run(
        ["git", "credential", "fill"], cwd=ROOT,
        input="protocol=https\nhost=github.com\n\n", capture_output=True, text=True, check=True,
    )
    values = dict(line.split("=", 1) for line in result.stdout.splitlines() if "=" in line)
    if not values.get("password"):
        raise RuntimeError("GitHub authentication unavailable")
    return values["password"]


def main():
    commit = git("rev-parse", "HEAD")
    if git("rev-parse", TAG + "^{}") != commit:
        raise RuntimeError("Release tag must match HEAD")
    remote = git("ls-remote", "origin", "refs/heads/main", "refs/tags/" + TAG + "^{}")
    if remote.count(commit) != 2:
        raise RuntimeError("Commit and annotated tag must already be pushed")
    if git("status", "--porcelain"):
        raise RuntimeError("Commit all source changes before publishing")
    checksums = {}
    for line in (OUTPUT / "SHA256SUMS.txt").read_text(encoding="utf-8").splitlines():
        digest, name = line.split("  ", 1)
        checksums[name] = digest
    for path in FILES:
        if not path.is_file():
            raise RuntimeError("Missing release artifact: " + path.name)
        if path.name in checksums and hashlib.sha256(path.read_bytes()).hexdigest() != checksums[path.name]:
            raise RuntimeError("Checksum mismatch: " + path.name)
    auth = token()

    def request(path, method="GET", value=None, content_type="application/json"):
        url = path if path.startswith("https://") else "https://api.github.com/repos/" + REPO + path
        if urllib.parse.urlparse(url).hostname not in ["api.github.com", "uploads.github.com"]:
            raise RuntimeError("Unexpected GitHub API host")
        data = value if isinstance(value, bytes) else json.dumps(value).encode("utf-8") if value is not None else None
        headers = {"Authorization": "Bearer " + auth, "Accept": "application/vnd.github+json",
                   "User-Agent": "AIDE-monitor-release", "X-GitHub-Api-Version": "2022-11-28", "Content-Type": content_type}
        with urllib.request.urlopen(urllib.request.Request(url, data=data, headers=headers, method=method), timeout=120) as response:
            return json.load(response)

    try:
        release = request("/releases/tags/" + TAG)
    except urllib.error.HTTPError as error:
        if error.code != 404:
            raise
        release = request("/releases", "POST", {
            "tag_name": TAG, "target_commitish": commit, "name": "AIDE monitor " + VERSION,
            "body": (ROOT / "docs/releases" / (TAG + ".md")).read_text(encoding="utf-8"),
            "draft": True, "prerelease": False,
        })
    assets = {asset["name"]: asset for asset in release["assets"]}
    upload_url = release["upload_url"].split("{", 1)[0]
    for path in FILES:
        digest = "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()
        if path.name in assets:
            asset = assets[path.name]
            if asset["size"] != path.stat().st_size or asset.get("digest") != digest:
                raise RuntimeError("Existing release asset differs: " + path.name)
            continue
        asset = request(upload_url + "?name=" + urllib.parse.quote(path.name), "POST", path.read_bytes(), "application/octet-stream")
        if asset["size"] != path.stat().st_size or asset.get("digest") != digest:
            raise RuntimeError("Uploaded release asset failed verification: " + path.name)
        print("Uploaded " + path.name, flush=True)
    release = request("/releases/" + str(release["id"]), "PATCH", {"draft": False, "make_latest": "true"})
    verified = request("/releases/tags/" + TAG)
    if verified["draft"] or verified["prerelease"] or len(verified["assets"]) != len(FILES):
        raise RuntimeError("Published release verification failed")
    print(json.dumps({"url": verified["html_url"], "tag": verified["tag_name"],
                      "assets": [{"name": a["name"], "size": a["size"], "digest": a.get("digest")} for a in verified["assets"]]}, ensure_ascii=False))


if __name__ == "__main__":
    main()
