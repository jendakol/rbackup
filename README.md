# RBackup
> Remote backup; Rust backup

## Motivation

Having a small home server (NAS), I had been using an application for backing up all important data from home computers. When the service
was announced to be about discontinued I've started to implement my own solution which is RBackup.

I also used this application as a great project for learning quite new but powerful language called [Rust](https://www.rust-lang.org/).

## Key features

1. Supports backup for multiple devices grouped in accounts // TBD - describe structure
1. Supports files versioning
1. Uses deduplication (block) (see [rdedup](https://github.com/dpc/rdedup))
1. Communicates over HTTP (the client can be almost any HTTP client)
1. Security - supports SSL, data encryption
1. Doesn't need any special storage (works on top of filesystem)

See [RBackup docs](https://jendakol.github.io/rbackup) for more information.
