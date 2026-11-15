#!/bin/zsh
# Publishes what package.sh made to the oby-downloads R2 bucket under electron/, which
# https://downloads.oby.dev/electron serves (docs/contracts/release.md, RELEASE-1;
# docs/contracts/updates.md): every zip latest-mac.yml names and the DMGs first, then latest-mac.yml
# last with Cache-Control: no-cache, so no Mac is sent to a zip that isn't there yet and a new
# release is seen at the next look. A feed that isn't signed, or names a zip that isn't there,
# uploads nothing.
# Usage: scripts/upload-release.sh [dist, by default]. Needs aws (the S3 CLI) with its credentials
# in AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY, and the bucket's account in OBY_R2_ENDPOINT.
set -euo pipefail

die() { echo "error: $*" >&2; exit 1; }
dist=${1:-dist}
feed=$dist/latest-mac.yml
bucket=s3://oby-downloads/electron
[[ -n "${OBY_R2_ENDPOINT:-}" ]] || die "OBY_R2_ENDPOINT isn't set: nothing is uploaded"
[[ -f "$feed" ]] || die "there's no $feed: nothing is uploaded"
grep -q '^signature: ' "$feed" || die "$feed isn't signed: nothing is uploaded"
zips=(${(f)"$(sed -n 's/^  - url: //p' "$feed")"})
(( ${#zips} )) || die "$feed names no zip: nothing is uploaded"
for zip in $zips; do
  [[ -f "$dist/$zip" ]] || die "$feed names $zip, which $dist doesn't have: nothing is uploaded"
done

put() { aws s3 cp --endpoint-url "$OBY_R2_ENDPOINT" --only-show-errors "$@"; }
for file in $zips "$dist"/*.dmg(N:t); do put "$dist/$file" "$bucket/$file"; done
put "$feed" "$bucket/latest-mac.yml" --cache-control no-cache --content-type text/yaml
echo "published ${zips[*]}, then latest-mac.yml"
