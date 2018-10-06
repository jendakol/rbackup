#!/usr/bin/env bash

function upload() {
    session_id=$1
    file_name=$2

    sha=$(sha256sum "$file_name" | awk '{ print $1 }')

    result=$(curl -sS --header "Content-Type: multipart/form-data" -H "RBackup-Session-Pass: ${session_id}" \
        -F file=@"${file_name}" -F file-hash="${sha}" \
        -X POST "http://localhost:3369/upload?file_path=theFileToBeUploaded.dat")

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
 && upload ${session_id} "theFileToBeUploaded.dat" > /dev/null \
 && upload ${session_id} "theFileToBeUploaded.dat" > /dev/null \
 && list_response=$(list_files ${session_id} | jq '.[] | {original_name: .original_name, versions: [.versions[] | { version: .version, hash: .hash, size: .size }] }') \
 && echo ${list_response} \
 && list_response_sha=$(echo ${list_response} | sha256sum | awk '{ print $1 }') \
 && assert "8dccd82e8e22115199801c700802456a6323c0a1e16927eeadddbbc79890584e" ${list_response_sha} "List response content was different" \
 && echo -e "\n\nTests were successful\n\n"

# SHA256 of (with trailing \n): { "original_name": "theFileToBeUploaded.dat", "versions": [ { "version": 1, "hash": "bc5ef071dd97166222168541bb53568b87e858b2db5614e120bc65fd6565f0af", "size": 1520 }, { "version": 2, "hash": "bc5ef071dd97166222168541bb53568b87e858b2db5614e120bc65fd6565f0af", "size": 1520 } ] }
