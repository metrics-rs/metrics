#!/usr/bin/env bash

while read PACKAGE_NAME PACKAGE_GIT_TAG; do
	if git diff --name-only $PACKAGE_GIT_TAG..HEAD | grep -q -E "^${PACKAGE_NAME}/"; then
		echo "$PACKAGE_NAME: changes since last release ($PACKAGE_GIT_TAG)";
	fi
done < <(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | "\(.name) \(.name)-v\(.version)"')