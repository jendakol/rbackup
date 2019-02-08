FROM jendakol/rbackup-base:latest

WORKDIR /tmp/rbackup
COPY . .

RUN export PATH="$HOME/.cargo/bin:$PATH" \
 && cargo install --path . \
 && apt-get remove -y curl file gcc pkg-config make clang-6.0 \
 && apt-get autoremove -y \
 && mv /root/.cargo/bin/rbackup /rbackup \
 && mv resources / \
 && rm -rf * /root/.cargo

ENTRYPOINT ["/rbackup", "-c", "/config.toml"]