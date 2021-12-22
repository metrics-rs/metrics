#!/usr/bin/env bash

PACKAGE_NAME=$1
PACKAGE_GIT_TAG=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | "\(.name)-v\(.version)"' | grep "${PACKAGE_NAME}-v")

echo "changes to $PACKAGE_NAME since last release ($PACKAGE_GIT_TAG):"
git diff $PACKAGE_GIT_TAG..HEAD $PACKAGE_NAME
