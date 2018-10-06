FROM rbackup:latest

WORKDIR /
COPY . .

ENTRYPOINT /wait-for-it.sh database:3306 -s -t 30 && /start-rbackup.sh

