#!/usr/bin/env bash
set -e

ADDR=127.0.0.1:9997
killall goto-api &> /dev/null || true

cargo build
target/debug/goto-api --addr ${ADDR} &

goto='target/debug/goto-cli'

echo 'Waiting for the GoTo API to be up and running'
for i in {1..20}
do
    curl --silent --max-time 1 --fail ${ADDR}/ > /dev/null && break || echo 'not ready...'
    sleep 1s
done

echo "Starting tests"
curl --silent --show-error --fail -X POST $ADDR/tsauvajon -d "https://linkedin.com/in/tsauvajon" > /dev/null
curl --silent --show-error --fail $ADDR/tsauvajon > /dev/null

curl --silent --show-error --fail -X POST $ADDR/hello -d "http://world" > /dev/null
curl --silent --show-error --fail $ADDR/hello > /dev/null

http_status=$(curl --silent -w "%{http_code}" $ADDR/qwertyuiop)

if [[ $http_status != "not found404" ]]; then
    echo "expected status 404, got $http_status"
    exit 2
fi

echo "SUCCESS"

killall goto-api
