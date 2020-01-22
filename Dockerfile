FROM debian:buster-slim

WORKDIR /tmp/rbackup
COPY . .

RUN apt-get update \
 && apt-get install -y curl file sudo gcc libsodium-dev clang-6.0 pkg-config make libssl-dev liblzma-dev \
 && curl https://sh.rustup.rs -sSf | sh -s -- --default-toolchain nightly-2019-07-02 -y \
 && export PATH="$HOME/.cargo/bin:$PATH" \
 && cargo build --release \
 && cargo install --path . \
 && apt-get remove -y curl file gcc pkg-config make clang-6.0 \
 && apt-get autoremove -y \
 && mv /root/.cargo/bin/rbackup /rbackup \
 && mv resources / \
 && rm -rf * /root/.cargo /root/.rustup /var/lib/apt/lists/*

ENTRYPOINT ["/rbackup", "-c", "/config.toml"]
