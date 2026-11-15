#!/bin/zsh
# Publishes what package.sh made to the oby-downloads R2 bucket (docs/contracts/release.md,
# RELEASE-1 and 4; docs/contracts/updates.md): every zip latest-mac.yml names and the DMGs first,
# then latest-mac.yml last with Cache-Control: no-cache, so no Mac is sent to a zip that isn't there
# yet and a new release is seen at the next look. A feed that isn't signed, or names a zip that
# isn't there, uploads nothing.
# Where it goes is the tag's: v<the feed's version> is a release, under electron/, which
# https://downloads.oby.dev/electron serves to every Mac; v<the feed's version>-<anything> is a test
# run, under electron-test/<tag>/, which no Mac reads; any other tag uploads nothing.
# Usage: scripts/upload-release.sh <tag> [dist, by default]. Needs aws (the S3 CLI) with its
# credentials in AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY, and the bucket's account in
# OBY_R2_ENDPOINT.
set -euo pipefail

die() { echo "error: $*" >&2; exit 1; }
tag=${1:?usage: upload-release.sh <tag> [dist]}
dist=${2:-dist}
feed=$dist/latest-mac.yml
[[ -n "${OBY_R2_ENDPOINT:-}" ]] || die "OBY_R2_ENDPOINT isn't set: nothing is uploaded"
[[ -f "$feed" ]] || die "there's no $feed: nothing is uploaded"
grep -q '^signature: ' "$feed" || die "$feed isn't signed: nothing is uploaded"
version=$(sed -n 's/^version: //p' "$feed")
if [[ -n "$version" && "$tag" == "v$version" ]]; then
  bucket=s3://oby-downloads/electron
elif [[ -n "$version" && "$tag" == "v$version-"?* && "$tag" != */* ]]; then
  bucket=s3://oby-downloads/electron-test/$tag
else
  die "the tag $tag is neither v$version (a release) nor v$version-<suffix> (a test run): nothing is uploaded"
fi
zips=(${(f)"$(sed -n 's/^  - url: //p' "$feed")"})
(( ${#zips} )) || die "$feed names no zip: nothing is uploaded"
for zip in $zips; do
  [[ -f "$dist/$zip" ]] || die "$feed names $zip, which $dist doesn't have: nothing is uploaded"
done

put() { aws s3 cp --endpoint-url "$OBY_R2_ENDPOINT" --only-show-errors "$@"; }
for file in $zips "$dist"/*.dmg(N:t); do put "$dist/$file" "$bucket/$file"; done
put "$feed" "$bucket/latest-mac.yml" --cache-control no-cache --content-type text/yaml
echo "published ${zips[*]}, then latest-mac.yml, to $bucket"
