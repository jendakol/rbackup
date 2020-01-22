#!/usr/bin/env bash

function wait_for_service() {
    n=0
    until [ $n -ge 30 ]
    do
      echo -e "Waiting for rbackup"

      if [ "$(curl "http://localhost:3369/status" 2>/dev/null | jq -r '.status' )" == "RBackup running" ]; then
        break
      fi

      n=$[$n+1]
      sleep 2
    done

    if [ $n -ge 30 ]; then
      docker-compose down
      exit 1
    fi

    echo -e "rbackup ready"
}

function set_correct_version {
    if [ -n "${TRAVIS_TAG}" ]; then
        echo "Set version to ${TRAVIS_TAG}"
        bash -c 'sed -i -r -e "s/^version = \"[0-9]+\.[0-9]+\.[0-9]+\"$/version = \""${TRAVIS_TAG}"\"/g" Cargo.toml'
    fi
}

function rbackup_test {
    set_correct_version && \
    docker build -t rbackup . && \
    cd tests && \
    docker-compose up -d --build --force-recreate && \
    wait_for_service && \
    ./tests.sh && \
    docker-compose down
}

function rbackup_publish {
    stripped_version=$(echo $TRAVIS_TAG | awk -F '[.]' '{print $1 "." $2}')

    docker tag rbackup jendakol/rbackup:$TRAVIS_TAG && \
    docker tag rbackup jendakol/rbackup:latest && \
    docker tag rbackup jendakol/rbackup:$stripped_version && \
    mkdir ~/.docker || true && \
    echo -e {\"auths\": {\"https://index.docker.io/v1/\": {\"auth\": \"${AUTH_TOKEN}\"}},\"HttpHeaders\": {\"User-Agent\": \"Travis\"}} > ~/.docker/config.json && \
    docker push jendakol/rbackup
}

sudo apt-get -qq update \
    && sudo apt-get install -y jq && \
    rbackup_test &&
      if $(test "${TRAVIS_REPO_SLUG}" == "jendakol/rbackup" && test "${TRAVIS_PULL_REQUEST}" == "false" && test "$TRAVIS_TAG" != ""); then
        rbackup_publish
      else
        exit 0 # skipping publish, it's regular build
      fi
