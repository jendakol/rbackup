# RBackup server

## Configuration

The configuration is split into multiple sections. Example config file is in [examples](https://github.com/jendakol/rbackup/blob/master/examples/Settings.toml).

// TBD

## Endpoints

All `<metadata>` are represented by an HTTP query string with fields described in _Request_ column.

All endpoints marked with `*` are authenticated. Authenticated endpoint requires `RBackup-Session-Pass` header to be provided. It's value is
_session_id_ retrieved by `GET /account/login`.

|Endpoint|Request|Response|Description|
|--------|-------|--------|-----------|
|GET `/status`|-|Status message|Health check|
|GET `/account/register?<metadata>`|string username, string password|- HTTP 201 with body _account_id_<br/>- HTTP 409 if account already exists|Registration of new account on the server|
|GET `/account/login?<metadata>`|string device_id, string username, string password|- HTTP 201 with body _session_id_ for new session<br/>- HTTP 200 with body _session_id_ for renewed session (this device already had a session, it was revoked and replaced by the new one, read more at [Session security](#session-security))<br/>- HTTP 401 if login was not successful|Login of session (connection of device to server)|
|GET* `/list/files?<metadata>`|string device_id (optional)|- HTTP 200 with [file list](#file-list) in body<br/>- HTTP 404 if device was not found|List all files currently held on server (for whole account or just for one device, if specified)|
|GET* `/list/devices`|-|- HTTP 200 with devices list in body (JSON array with strings)|List all devices of account related to the session|
|GET* `/download?<metadata>`|int file_version_id|- HTTP 200 with `Content-Length` and `RBackup-File-Hash` headers and file bytes in body (chunked)<br/>- HTTP 404 if there is no such file available for download|Download file from server, providing it's version id|
|PUT* `/upload?<metadata>`|Query: string file_path, long size, long mtime<br/>Body: see [file upload](#file-upload) section|- HTTP 200 with [file](#file) in body<br/>- HTTP 412 if calculated hash of received data does not match the provided one<br/>- HTTP 400 if the request is invalid|Upload the file|
|DELETE* `/remove/file?<metadata>`|int file_id|- HTTP 200 if the file was deleted<br/>- HTTP 404 if there is no such file|Delete file from server|
|DELETE* `/remove/fileVersion?<metadata>`|int file_version_id|- HTTP 200 if the file version was deleted<br/>- HTTP 404 if there is no such file version|Delete particular version of file|

Please note that all endpoints may return HTTP 500 or similar in case of unexpected failure.

### File
JSON representation of uploaded file.  
Example:

```json
{
    "id": 2583,
    "device_id": "file1",
    "original_name": "config1",
    "versions": [
      {
        "version": 1,
        "size": 354,
        "hash": "d74643823048ffd090ecf342208d49253ec4b9f3acd5c47f6bb526e3fb67f544",
        "created": "2018-07-24T19:15:43",
        "storage_name": "562ea86d62eac4df8dc7c3ff700e2f4c2dec5dccf235409a695e919a5c02ea44"
      },
      {
        "version": 2,
        "size": 354,
        "hash": "d74643823048ffd090ecf342208d49253ec4b9f3acd5c47f6bb526e3fb67f544",
        "created": "2018-07-24T19:15:44",
        "storage_name": "a385dc3de9e5834a5e28b32ca59ff54f60fd6ee3862ee6a84b3064f252346f76"
      }
    ]
  }
```

### File list

The file list has following JSON structure:
* Root contains array of files
* Each file has some attributes and array of versions
* Each version has some attributes

Example:
```json
[
  {
    "id": 2583,
    "device_id": "file1",
    "original_name": "config1",
    "versions": [
      {
        "version": 1,
        "size": 354,
        "hash": "d74643823048ffd090ecf342208d49253ec4b9f3acd5c47f6bb526e3fb67f544",
        "created": "2018-07-24T19:15:43",
        "storage_name": "562ea86d62eac4df8dc7c3ff700e2f4c2dec5dccf235409a695e919a5c02ea44"
      },
      {
        "version": 2,
        "size": 354,
        "hash": "d74643823048ffd090ecf342208d49253ec4b9f3acd5c47f6bb526e3fb67f544",
        "created": "2018-07-24T19:15:44",
        "storage_name": "a385dc3de9e5834a5e28b32ca59ff54f60fd6ee3862ee6a84b3064f252346f76"
      }
    ]
  },
  {
    "id": 2584,
    "device_id": "file2",
    "original_name": "config2",
    "versions": [
      {
        "version": 3,
        "size": 354,
        "hash": "d74643823048ffd090ecf342208d49253ec4b9f3acd5c47f6bb526e3fb67f544",
        "created": "2018-07-24T19:15:43",
        "storage_name": "562ea86d62eac4df8dc7c3ff700e2f4c2dec5dccf235409a695e919a5c02ea45"
      },
      {
        "version": 4,
        "size": 354,
        "hash": "d74643823048ffd090ecf342208d49253ec4b9f3acd5c47f6bb526e3fb67f544",
        "created": "2018-07-24T19:15:44",
        "storage_name": "a385dc3de9e5834a5e28b32ca59ff54f60fd6ee3862ee6a84b3064f252346f77"
      }
    ]
  }
]
```

The list may or may not contain files from multiple devices (based on providing particular `device_id`).

### File upload

For the file upload the [multipart/form-data](https://stackoverflow.com/questions/16958448/what-is-http-multipart-request) is used with
following sections:
1. `file` - raw file bytes
1. `file-hash` - SHA256 hash of the file being sent (hex format)

Speaking in terms of `bash and curl`:
```bash
sha=$(sha256sum "$file_name" | awk '{ print $1 }')

curl -sS --header "Content-Type: multipart/form-data" -H "RBackup-Session-Pass: ${session_id}" \
        -F file=@"${file_name}" -F file-hash="${sha}" \
        -X POST "${server}/upload?file_name=${file_name}"
```

### Session security

// TBD
