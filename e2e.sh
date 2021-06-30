#!/usr/bin/env bash
set -e

ADDR=127.0.0.1:9997
killall goto-api &> /dev/null || true

cargo build
target/debug/goto-api --addr ${ADDR} &

cargo build --bin goto

echo 'Waiting for the Goto API to be up and running'
for i in {1..20}
do
    curl --silent --max-time 1 --fail ${ADDR}/ > /dev/null && break || echo 'not ready...'
    sleep 1s
done

echo -e 'Starting tests\n'

echo 'creating /tsauvajon with a direct HTTP query'
curl --silent --show-error --fail -X POST $ADDR/tsauvajon -d "https://linkedin.com/in/tsauvajon" > /dev/null
echo -e ' -> ok\n'
echo 'browsing /tsauvajon with the CLI'
target/debug/goto tsauvajon --no-open-browser --api http://${ADDR} | grep -q 'redirecting to https://linkedin.com/in/tsauvajon' || (echo ' -> failed!' && exit 2)
echo -e ' -> ok\n'

echo 'creating /hello with the CLI'
target/debug/goto hello http://hello.world --api http://${ADDR}
echo -e ' -> ok\n'
echo 'browsing /hello with a direct HTTP query'
curl --silent --show-error --fail $ADDR/hello | grep -q 'redirecting to http://hello.world' || (echo ' -> failed!' && exit 3)
echo -e ' -> ok\n'

echo 'querying inexisting short URL'
http_status=$(curl --silent -w "%{http_code}" $ADDR/qwertyuiop)

if [[ $http_status != "not found404" ]]; then
    echo " -> expected status 404, got '$http_status'"
    echo ' -> failed!'
    exit 404
fi

echo -e ' -> ok\n'
echo 'SUCCESS'

killall goto-api
