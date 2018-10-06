#!/usr/bin/env bash

function wait_for_service() {
    n=0
    until [ $n -ge 5 ]
    do
      echo -e "Waiting for rbackup"
      curl "http://localhost:3369/status" 2> /dev/null > /dev/null && break
      n=$[$n+1]
      sleep 1
    done

    echo -e "rbackup ready"
}

function rbackup_test {
     docker build -t rbackup . \
     && cd tests \
     && docker-compose up -d --build --force-recreate \
     && wait_for_service \
     && ./tests.sh \
     && docker-compose down
}

function rbackup_publish {
    stripped_version=$(echo $TRAVIS_TAG | awk -F '[.]' '{print $1 "." $2}')

    docker tag rbackup jendakol/rbackup:$TRAVIS_TAG
    docker tag rbackup jendakol/rbackup:latest
    docker tag rbackup jendakol/rbackup:$stripped_version
    mkdir ~/.docker || true
    echo -e {\"auths\": {\"https://index.docker.io/v1/\": {\"auth\": \"${AUTH_TOKEN}\"}},\"HttpHeaders\": {\"User-Agent\": \"Travis\"}} > ~/.docker/config.json
    docker push jendakol/rbackup
}

sudo apt-get -qq update \
    && sudo apt-get install -y jq && \
    rbackup_test &&
      if $(test ${TRAVIS_REPO_SLUG} == "jendakol/rbackup" && test ${TRAVIS_PULL_REQUEST} == "false" && test "$TRAVIS_TAG" != ""); then
        rbackup_publish
      else
        exit 0 # skipping publish, it's regular build
      fi