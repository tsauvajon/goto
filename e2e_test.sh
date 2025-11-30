#!/usr/bin/env bash
set -e

# make replace
# make install

GREEN='\033[32m'
RED='\033[31m'
RESET='\033[0m'

if goto hello http://world; then
    printf "create or replace ... ${GREEN}ok${RESET}\n"
else
    printf "could not create or replace URL ... ${RED}FAILED${RESET}\n"
    exit 2
fi

if goto hello http://planet --force; then
    printf "replace ... ${GREEN}ok${RESET}\n"
else
    printf "could not replace URL ... ${RED}FAILED${RESET}\n"
    exit 3
fi

if goto hello http://earth 2>/dev/null; then
    printf "expected error but it succeeded ... ${RED}FAILED${RESET}\n"
    exit 4
else
    printf "does not replace by default ... ${GREEN}ok${RESET}\n"
fi

output=$(goto hello --no-open-browser)
if echo "$output" | grep -q 'redirecting to http://planet'; then
    printf "final target ... ${GREEN}ok${RESET}\n"
else
    printf "expected http://planet, got %s ... ${RED}FAILED${RESET}\n" "$output"
    exit 5
fi
