# RBackup
> Remote backup; Rust backup

## Motivation

Having a small home server (NAS), I had been using an application for backing up all important data from home computers. When the service
was announced to be about discontinued I've started to implement my own solution which is RBackup.

## Key features

1. Supports backup for multiple devices grouped in accounts
1. Supports files versioning
1. Uses block deduplication (see [rdedup](https://github.com/dpc/rdedup))
1. Communicates over HTTP (the client can be almost any HTTP client, incl. [cURL](https://en.wikipedia.org/wiki/CURL))
1. Security - supports SSL, data encryption on disk
1. Doesn't need any special storage (works on top of the filesystem)

See the documentation for the [server](server.md) and the [Scala client](scala-client.md) for more information.

// TBD describe features more deeply
