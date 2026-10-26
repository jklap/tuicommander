#!/usr/bin/env bash

# Terminal colors
OFF='\033[0m'
RED='\033[0;31m'
BRIGHT_RED='\033[0;91m'
BRIGHT_YELLOW='\033[0;93m'
GREEN='\033[0;32m'
BLUE='\033[0;94m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD_RED='\033[1;31m'
BOLD_GREEN='\033[1;32m'
BOLD_BLUE='\033[1;34m'
BOLD_PURPLE='\033[1;35m'
BOLD_CYAN='\033[1;36m'
BOLD_YELLOW='\033[1;33m'
BOLD_UNDERLINED='\033[1;4m'
BOLD='\033[1m'

CHECK='✓'
CROSS='✗'
DOT='•'

log() {
    local NOT_OK=$1
    local MSG="$2"
    local FAILED_MSG="${3:-$MSG}"
    if [ $1 -ne 0 ]; then
        echo -e "  ${BOLD_RED}${CROSS}${OFF} ${FAILED_MSG}"
    else
        echo -e "  ${BOLD_GREEN}${CHECK}${OFF} ${MSG}"
    fi
}

check_folder() {
    local FOLDER="$1"
    #echo "${FOLDER} folder"
    ls "$FOLDER" >& /dev/null
    log $? "$FOLDER"
}

echo "Data from other apps"
op account list >& /dev/null
log $? "App data access" "Failed to read from 1Password"

# unsigned software
# x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_DevTools

test_script() {
    local APP="$1"
    local SCRIPT="$2"
    osascript -e "$SCRIPT" > /dev/null
    log $? "$APP"
}

echo "Automation"
# x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Automation
# System Events
# BBEdit
# IntelliJ
# Safari
# osascript -e 'tell application "System Events"' -e 'tell application process "IntelliJ IDEA"' -e 'set testCheck to name of front window' -e 'end tell' -e 'end tell'
test_script "Finder" 'tell application "Finder" to get name of front Finder window'

# below needs to be enabled manually via accessibility
# osascript -e 'tell application "System Events" to key code 63'

# photos
# x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Photos
echo "Photos"
check_folder "$HOME/Pictures/Photos Library.photoslibrary"

# media
# x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Media
echo "Media"
check_folder "$HOME/Music/Music/"

echo "Microphone"
# x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Microphone
if ! [ -f "trigger_mic" ]; then
    if ! [ -f "trigger_mic.swift" ]; then
        cat << 'EOF' > trigger_mic.swift
import AVFoundation
import Foundation

// Force the system call by requesting access asynchronously (triggers the OS popup if needed)
AVCaptureDevice.requestAccess(for: .audio) { granted in
    // This closure will fire once the user clicks an option or instantly if already decided
}

// Read the absolute system status immediately to determine the exit code
let status = AVCaptureDevice.authorizationStatus(for: .audio)

switch status {
case .authorized:
    print("Permission GRANTED")
    exit(0)
case .denied:
    print("Permission DENIED")
    exit(1)
case .notDetermined:
    print("Permission NOT DETERMINED (Prompt triggered, waiting for user response)")
    // Give the user a moment to interact with the OS popup before exiting
    sleep(5)
    exit(2)
case .restricted:
    print("Permission RESTRICTED (e.g., Parental Controls/MDM)")
    exit(3)
@unknown default:
    exit(4)
}
EOF
    fi
    swiftc trigger_mic.swift
fi
./trigger_mic >& /dev/null
log $? "Microphone access"

# other folders?

# x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_FilesAndFolders
echo "Folders"
check_folder "$HOME/Downloads"
check_folder "$HOME/Documents"
check_folder "$HOME/Desktop"

# TODO: network access
# ping??

echo "Full Disk Access"
# TODO: full disk support -- do we want this actually since we are running agents?
# x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AllFiles
check_folder "$HOME/Library/Safari"

# modify other apps
# x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AppBundles
echo "Modify Apps"
touch /Applications/Maccy.app/Contents/Info.plist >& /dev/null
log $? "Able to modify apps" "No access"

# accessibility
# x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility

# Notifications
# x-apple.systempreferences:com.apple.preference.notifications

exit 0
