#!/usr/bin/env sh

nomad agent -dev -network-interface="en0" &
export NOMAD_ADDR='http://localhost:4646'
waypoint install -platform=nomad -nomad-dc=dc1 -accept-tos
waypoint init
