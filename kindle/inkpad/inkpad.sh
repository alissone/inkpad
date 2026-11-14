#!/bin/sh
# inkpad launcher: takes the screen from the stock Kindle UI, runs an inkpad
# binary, and ALWAYS gives the screen back, even if the binary crashes.
# Usage: inkpad.sh <binary> [args...]   (path relative to /mnt/us/inkpad)
# The takeover steps mirror what KOReader's launcher does on firmware 5.x.
#
# Handoff: if the binary exits with code 42 and wrote /var/tmp/inkpad-handoff
# ("koreader", a file to open, an app id to return to; one per line), we
# give the screen back, run KOReader until it closes, then start home again
# (reopening that app) instead of returning to the Kindle UI.

DIR=/mnt/us/inkpad
LOG="$DIR/inkpad.log"
BIN="$DIR/$1"
shift
HANDOFF=/var/tmp/inkpad-handoff
KOREADER=/mnt/us/koreader/koreader.sh
FB_DUMP=/var/tmp/inkpad-fb.dump

# Keep the log small.
[ -f "$LOG" ] && [ "$(wc -c < "$LOG")" -gt 200000 ] && mv -f "$LOG" "$LOG.old"
exec >> "$LOG" 2>&1
echo "=== $(date) start $BIN $*"

PILLOW_DISABLED=no
AWESOME_STOPPED=no
VOLUMD_STOPPED=no

# Hide the stock status bar/dialogs, freeze the window manager so it can't
# draw over us, and pause volumd so USB storage can't pull the disk away.
take_screen() {
    lipc-set-prop com.lab126.pillow disableEnablePillow disable && PILLOW_DISABLED=yes
    killall -STOP awesome && AWESOME_STOPPED=yes
    killall -STOP volumd && VOLUMD_STOPPED=yes
    usleep 300000
}

give_screen() {
    [ "$VOLUMD_STOPPED" = yes ] && killall -CONT volumd
    [ "$AWESOME_STOPPED" = yes ] && killall -CONT awesome
    [ "$PILLOW_DISABLED" = yes ] && lipc-set-prop com.lab126.pillow disableEnablePillow enable
    VOLUMD_STOPPED=no
    AWESOME_STOPPED=no
    PILLOW_DISABLED=no
}

restore() {
    give_screen
    # Put back the screen as it was before we started: the stock status bar
    # only repaints the parts that change (the clock), so without this it
    # would keep showing our pixels.
    if [ -f "$FB_DUMP" ]; then
        cat "$FB_DUMP" > /dev/fb0
        rm -f "$FB_DUMP"
    fi
    rm -f "$HANDOFF"
    # Bring the stock home screen back so it repaints everything.
    lipc-set-prop com.lab126.appmgrd start app://com.lab126.booklet.home
    echo "=== $(date) restored"
}
trap restore EXIT
trap 'exit 1' INT TERM HUP

if [ ! -f "$BIN" ]; then
    echo "missing binary: $BIN"
    exit 1
fi
chmod +x "$BIN"

cat /dev/fb0 > "$FB_DUMP"

while true; do
    rm -f "$HANDOFF"
    take_screen
    "$BIN" "$@"
    CODE=$?
    echo "exit code $CODE"
    [ "$CODE" -eq 42 ] && [ -f "$HANDOFF" ] || break

    TARGET=$(sed -n 1p "$HANDOFF")
    FILE=$(sed -n 2p "$HANDOFF")
    RETURN_TO=$(sed -n 3p "$HANDOFF")
    rm -f "$HANDOFF"
    if [ "$TARGET" = kindle ] && [ -f "$FILE" ]; then
        # Open in the stock reader: give everything back, then ask the
        # Kindle's app manager for the book. (EXPERIMENTAL: URI form taken
        # from community tools, not yet verified on this firmware.)
        echo "=== $(date) handoff to the Kindle reader: $FILE"
        trap - EXIT
        give_screen
        rm -f "$FB_DUMP"
        lipc-set-prop com.lab126.appmgrd start "app://com.lab126.booklet.reader$FILE" ||
            lipc-set-prop com.lab126.appmgrd start app://com.lab126.booklet.home
        exit 0
    fi
    [ "$TARGET" = koreader ] && [ -f "$KOREADER" ] || break

    give_screen
    echo "=== $(date) handoff to KOReader: ${FILE:-<file browser>}"
    chmod +x "$KOREADER" 2>/dev/null
    if [ -n "$FILE" ]; then
        "$KOREADER" "$FILE"
    else
        "$KOREADER"
    fi
    echo "=== $(date) KOReader closed, back to inkpad"

    # Back to home (skipping the PIN), reopening the app that asked.
    BIN="$DIR/home"
    if [ -n "$RETURN_TO" ]; then
        set -- --resume --launch "$RETURN_TO"
    else
        set -- --resume
    fi
done
