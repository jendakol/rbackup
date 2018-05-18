#!/usr/bin/env bash

function upload() {
    session_id=$1
    file_name=$2

    sha=$(sha256sum "$file_name" | awk '{ print $1 }')

    curl -sS --header "Content-Type: multipart/form-data" -H "RBackup-Session-Pass: ${session_id}" \
        -F file=@"${file_name}" -F file-hash="${sha}" \
        -X POST "http://localhost:3369/upload?file_name=config"
}

function list_files() {
    session_id=$1

    curl -sS -H "RBackup-Session-Pass: ${session_id}" -X GET "http://localhost:3369/list/files/?"
}

function assert() {
    expected=$1
    actual=$2
    hint=$3

    test "${expected}" = "${actual}" || echo "${hint}"
}

echo -e "Running tests:\n"

curl -sS "http://localhost:3369/account/register?username=rbackup&password=rbackup" > /dev/null \
 && session_id=$(curl -sS "http://localhost:3369/account/login?device_id=docker-tests&username=rbackup&password=rbackup") \
 && echo -e "SessionID: ${session_id} \n" \
 && upload ${session_id} config.toml > /dev/null \
 && second_response=$(upload ${session_id} config.toml) \
 && list_response_sha=$(list_files ${session_id} | jq '.[] | {original_name: .original_name, versions: [.versions[] | { version: .version, hash: .hash, size: .size }] }' | sha256sum | awk '{ print $1 }') \
 && assert "aae65f1df784e1ef2c9e12da0eaf78429044d32f1d5ed961d5e1ed4436bd9a89" ${list_response_sha} "List response content was different" \
 && echo -e "\n\nTests were successful\n\n"
