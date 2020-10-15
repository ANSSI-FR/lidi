#!/bin/sh

cp target/release/lidi-up* packages/lidi-up
cp target/release/lidi-down* packages/lidi-down

cd packages/lidi-down
dpkg-buildpackage -us -uc

cd ../../packages/lidi-up
dpkg-buildpackage -us -uc