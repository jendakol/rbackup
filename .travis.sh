#!/usr/bin/env bash

function wait_for_service() {
    until curl "http://localhost:3369/status" 2> /dev/null > /dev/null ; do
      echo -e "Waiting for rbackup"
      sleep 1
    done

    echo -e "rbackup ready"
}

function rbackup_test {
    docker build -t rbackup . \
     && cd tests \
     && docker-compose up -d \
     && wait_for_service \
     && ./tests.sh \
     && docker-compose down \
     || docker-compose down
}

function rbackup_publish {
    docker tag rbackup jendakol/rbackup:$TRAVIS_TAG
    echo -e {\"auths\": {\"https://index.docker.io/v1/\": {\"auth\": \"${AUTH_TOKEN}\"}},\"HttpHeaders\": {\"User-Agent\": \"Travis\"}} > ~/.docker/config.json
    docker push jendakol/rbackup:$TRAVIS_TAG
}

rbackup_test &&
  if $(test ${TRAVIS_REPO_SLUG} == "jendakol/rbackup" && test ${TRAVIS_PULL_REQUEST} == "false" && test "$TRAVIS_TAG" != ""); then
    rbackup_publish
  else
    exit 0 # skipping publish, it's regular build
  fi