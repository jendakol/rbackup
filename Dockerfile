FROM debian:9-slim

WORKDIR /tmp/rbackup
COPY . .

RUN apt-get update && apt-get install -y curl file sudo gcc libsodium18 libsodium-dev pkg-config make libssl-dev liblzma-dev \
 && curl -f -L https://static.rust-lang.org/rustup.sh -O \
 && /bin/bash rustup.sh --channel=nightly --date=2018-09-26 \
 && cargo install \
 && /bin/bash rustup.sh --channel=nightly --uninstall \
 && apt-get remove -y curl file gcc pkg-config make \
 && apt-get autoremove -y \
 && mv /root/.cargo/bin/rbackup /rbackup \
 && mv resources / \
 && rm -rf * /root/.cargo

ENTRYPOINT ["/rbackup", "-c", "/config.toml"]