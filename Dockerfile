FROM jendakol/rbackup-base:latest

WORKDIR /tmp/rbackup
COPY . .

RUN cargo install --path . \
 && /bin/bash rustup.sh --channel=nightly --uninstall \
 && apt-get remove -y curl file gcc pkg-config make clang-6.0 \
 && apt-get autoremove -y \
 && mv /root/.cargo/bin/rbackup /rbackup \
 && mv resources / \
 && rm -rf * /root/.cargo

ENTRYPOINT ["/rbackup", "-c", "/config.toml"]