#!/usr/bin/env bash
set -e

ADDR=127.0.0.1:9997
killall goto-api &> /dev/null || true

GREEN='\033[32m'
RED='\033[31m'
RESET='\033[0m'

cargo build
target/debug/goto-api --addr ${ADDR} &

cargo build --bin goto

echo 'Waiting for the Goto API to be up and running'
for i in {1..20}
do
    curl --silent --max-time 1 --fail ${ADDR}/ > /dev/null && break || echo 'not ready...'
    sleep 1s
done

printf 'Starting tests\n\n'

echo 'creating /tsauvajon with a direct HTTP query'
curl --silent --show-error --fail -X POST $ADDR/tsauvajon -d "https://linkedin.com/in/tsauvajon" > /dev/null
printf " -> ${GREEN}ok${RESET}\n\n"
echo 'browsing /tsauvajon with the CLI'
target/debug/goto tsauvajon --no-open-browser --api http://${ADDR} | grep -q 'redirecting to https://linkedin.com/in/tsauvajon' || (printf " -> ${RED}failed!${RESET}\n" && exit 2)
printf " -> ${GREEN}ok${RESET}\n\n"

echo 'creating /hello with the CLI'
target/debug/goto hello http://hello.world --api http://${ADDR}
printf " -> ${GREEN}ok${RESET}\n\n"
echo 'browsing /hello with a direct HTTP query'
curl --silent --show-error --fail $ADDR/hello | grep -q 'redirecting to http://hello.world' || (printf " -> ${RED}failed!${RESET}\n" && exit 3)
printf " -> ${GREEN}ok${RESET}\n\n"

echo 'querying inexisting short URL'
http_status=$(curl --silent -w "%{http_code}" $ADDR/qwertyuiop)

if [[ $http_status != "not found404" ]]; then
    echo " -> expected status 404, got '$http_status'"
    printf " -> ${RED}failed!${RESET}\n"
    exit 404
fi

printf " -> ${GREEN}ok${RESET}\n\n"
printf "${GREEN}SUCCESS${RESET}\n"

killall goto-api
