#!/usr/bin/env bash

function upload() {
    session_id=$1
    file_name=$2

    sha=$(sha256sum "$file_name" | awk '{ print $1 }')

    result=$(curl -sS --header "Content-Type: multipart/form-data" -H "RBackup-Session-Pass: ${session_id}" \
        -F file=@"${file_name}" -F file-hash="${sha}" \
        -X POST "http://localhost:3369/upload?file_path=config")

    if  [[ ${result} == Failure* ]];
    then
        echo ${result}
        exit 1
    fi

    if  [[ ${result} == Cannot* ]];
    then
        echo ${result}
        exit 1
    fi

    echo ${result}
}

function list_files() {
    session_id=$1

    result=$(curl -sS -H "RBackup-Session-Pass: ${session_id}" -X GET "http://localhost:3369/list/files/?")

    if  [[ ${result} == Failure* ]];
    then
        echo ${result}
        exit 1
    fi

    if  [[ ${result} == Cannot* ]];
    then
        echo ${result}
        exit 1
    fi

    echo ${result}
}

function assert() {
    expected=$1
    actual=$2
    hint=$3

    test "${expected}" = "${actual}" || (echo "${hint} (expected ${expected}, actual ${actual})" && exit 1)
}

echo -e "Running tests:\n"

curl -sS "http://localhost:3369/account/register?username=rbackup&password=rbackup" > /dev/null \
 && session_id=$(curl -sS "http://localhost:3369/account/login?device_id=docker-tests&username=rbackup&password=rbackup" | jq '.session_id' | sed -e 's/^"//' -e 's/"$//') \
 && echo -e "SessionID: ${session_id} \n" \
 && upload ${session_id} config.toml > /dev/null \
 && upload ${session_id} config.toml > /dev/null \
 && list_response=$(list_files ${session_id} | jq '.[] | {original_name: .original_name, versions: [.versions[] | { version: .version, hash: .hash, size: .size }] }') \
 && echo ${list_response} \
 && list_response_sha=$(echo ${list_response} | sha256sum | awk '{ print $1 }') \
 && assert "e4051187f0b36c8a1a952f83c7d19e093abc7bf52241445419a213ef8e096e29" ${list_response_sha} "List response content was different" \
 && echo -e "\n\nTests were successful\n\n"

# SHA256 of (with trailing \n): { "original_name": "config", "versions": [ { "version": 1, "hash": "d74643823048ffd090ecf342208d49253ec4b9f3acd5c47f6bb526e3fb67f544", "size": 354 }, { "version": 2, "hash": "d74643823048ffd090ecf342208d49253ec4b9f3acd5c47f6bb526e3fb67f544", "size": 354 }, { "version": 3, "hash": "d74643823048ffd090ecf342208d49253ec4b9f3acd5c47f6bb526e3fb67f544", "size": 354 }, { "version": 4, "hash": "d74643823048ffd090ecf342208d49253ec4b9f3acd5c47f6bb526e3fb67f544", "size": 354 } ] }

