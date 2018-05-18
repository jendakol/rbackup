FROM rbackup:latest

WORKDIR /
COPY . .

RUN apt-get install -y jq

ENTRYPOINT /wait-for-it.sh database:3306 -s -t 30 && /rbackup -c /config.toml dbinit && /rbackup -c /config.toml

