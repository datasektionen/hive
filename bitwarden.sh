#!/bin/bash

./bw config server $BW_SERVER
./bw login --apikey
./bw serve --hostname $BW_HOST --port $BW_PORT
