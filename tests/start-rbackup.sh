#!/usr/bin/env bash

n=0
until [ $n -ge 5 ]
do
   echo "Starting rbackup"
   /rbackup -c /config.toml dbinit && /rbackup -c /config.toml && break
   n=$[$n+1]
   sleep 5
done
