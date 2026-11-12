#!/bin/sh
# Name: Inkpad hello test
# Author: inkpad
# DontUseFBInk
# M0 test: runs the hello binary and shows the result on screen.
DIR=/mnt/us/inkpad
LOG=$DIR/hello.log
chmod +x "$DIR/hello" 2>/dev/null
"$DIR/hello" > "$LOG" 2>&1
RC=$?
echo "exit_code: $RC" >> "$LOG"
eips 1 36 "inkpad hello: exit $RC (see inkpad/hello.log)"
