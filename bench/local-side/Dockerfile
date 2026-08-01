# syntax=docker/dockerfile:1
FROM bench-common

ARG UPLOAD_FILENAME="upload.dat"

COPY ./local_side.py /bench/

WORKDIR /bench/
RUN dd bs=$((1024 * 1024)) count=1024 if=/dev/zero of="$UPLOAD_FILENAME"

ENV UPLOAD_FILENAME=$UPLOAD_FILENAME
