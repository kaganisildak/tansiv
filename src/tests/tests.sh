#!/usr/bin/env bash

./tests --list-test-names-only | xargs -d "\n" -n1  ./tests

