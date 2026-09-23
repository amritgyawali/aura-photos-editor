"""Anonymous public-photo retrieval. No browser cookies or account credentials are read."""
import contextlib
import json
import pathlib
import re
import sys
from urllib.parse import urlparse


def fetch(handle, folder, limit):
    import instaloader

    root = pathlib.Path(folder)
    root.mkdir(parents=True, exist_ok=True)
    loader = instaloader.Instaloader(quiet=True, max_connection_attempts=1, request_timeout=15)
    fetched = 0
    skipped = 0
    complete = False
    total_bytes = 0
    message = ""
    try:
        profile = instaloader.Profile.from_username(loader.context, handle)
        if profile.is_private:
            raise RuntimeError("This profile is private. Add saved reference photos instead.")
        for post in profile.get_posts():
            if fetched >= limit:
                break
            if not re.fullmatch(r"[A-Za-z0-9_-]+", post.shortcode):
                skipped += 1
                continue
            media = [(post.url, post.is_video)] if post.typename != "GraphSidecar" else [
                (node.display_url, node.is_video) for node in post.get_sidecar_nodes()
            ]
            for index, (url, video) in enumerate(media):
                if fetched >= limit:
                    break
                if video:
                    skipped += 1
                    continue
                parsed = urlparse(url)
                if parsed.scheme != "https" or not any(
                    (parsed.hostname or "").endswith(domain) for domain in (".cdninstagram.com", ".fbcdn.net")
                ):
                    skipped += 1
                    continue
                target = root / f"{post.shortcode}_{index}.jpg"
                temporary = target.with_suffix(".part")
                try:
                    size = 0
                    with loader.context.get_raw(url) as response, temporary.open("wb") as output:
                        for chunk in response.iter_content(64 * 1024):
                            size += len(chunk)
                            if size > 16 * 1024 * 1024:
                                raise RuntimeError("Reference photo exceeds the 16 MB download limit")
                            if total_bytes + size > 512 * 1024 * 1024:
                                raise RuntimeError("Stopped at the 512 MB reference download limit. Analyze the downloaded sample or choose a smaller set.")
                            output.write(chunk)
                    if size == 0:
                        raise RuntimeError("Empty photo response")
                    temporary.replace(target)
                    fetched += 1
                    total_bytes += size
                finally:
                    temporary.unlink(missing_ok=True)
            # Keep the cache bounded even for an account with thousands of posts.
            if fetched >= limit:
                break
        else:
            complete = True
        message = "Reached the end of the accessible posts." if complete else f"Stopped at the {limit}-photo limit."
    except Exception as error:
        message = str(error)
        if any(word in message.lower() for word in ("401", "403", "429", "login", "wait a few", "checkpoint")):
            message = "Instagram limited public access or requires a login. Try later, or add a folder of saved reference photos."
    return {"folder": str(root), "fetched": fetched, "skipped": skipped, "complete": complete, "message": message[:600]}


if __name__ == "__main__":
    try:
        with contextlib.redirect_stdout(sys.stderr):
            result = fetch(sys.argv[1], sys.argv[2], min(2000, max(8, int(sys.argv[3]))))
    except ImportError:
        result = {"folder": sys.argv[2], "fetched": 0, "skipped": 0, "complete": False,
                  "message": "Instagram retrieval needs Python with Instaloader installed. Use saved reference photos, or install with: python -m pip install instaloader"}
    print(json.dumps(result))
