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

check_folder() {
    local FOLDER="$1"
    #echo "${FOLDER} folder"
    ls "$FOLDER" >& /dev/null
    if [ $? -ne 0 ]; then
        echo -e "  ${BOLD_RED}${CROSS}${OFF} ${FOLDER}"
    else
        echo -e "  ${BOLD_GREEN}${CHECK}${OFF} ${FOLDER}"
    fi
}

echo "Data from other apps"
op account list >& /dev/null
if [ $? -ne 0 ]; then
    echo -e "  ${BOLD_RED}${CROSS}${OFF} Failed to read from 1Password"
else
    echo -e "  ${BOLD_GREEN}${CHECK}${OFF} App data access"
fi

# unsigned software
# x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_DevTools

test_script() {
    local APP="$1"
    local SCRIPT="$2"
    osascript -e "$SCRIPT" > /dev/null
    if [ $? -ne 0 ]; then
        echo -e "  ${BOLD_RED}${CROSS}${OFF} ${APP}"
    else
        echo -e "  ${BOLD_GREEN}${CHECK}${OFF} ${APP}"
    fi
}

echo "Automation"
# x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Automation
# System Events
# BBEdit
# IntelliJ
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

# TODO: microphone
# x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Microphone
# ffmpeg -f avfoundation -i ":0" -t 1 out.wav
# sox -d -d

# other folders?

# x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_FilesAndFolders
echo "Folders"
check_folder "$HOME/Downloads"
check_folder "$HOME/Documents"
check_folder "$HOME/Desktop"

# TODO: network access
# ping??

# TODO: full disk support -- do we want this actually since we are running agents?
# x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AllFiles
check_folder "$HOME/Library/Safari"

# modify other apps
# x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AppBundles
echo "Modify Apps"
touch /Applications/Maccy.app/Contents/Info.plist >& /dev/null
if [ $? -ne 0 ]; then
    echo -e "  ${BOLD_RED}${CROSS}${OFF} No access"
else
    echo -e "  ${BOLD_GREEN}${CHECK}${OFF} Able to modify apps"
fi

# accessibility
# x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility

# Notifications
# x-apple.systempreferences:com.apple.preference.notifications

exit 0
