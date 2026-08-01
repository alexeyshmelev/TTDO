# syntax=docker/dockerfile:1
FROM nginx:1.31.3@sha256:5a88c9c45479443d7be2eadc894b4ed0a9801bae03d97a5760ae13b5c2005942

ARG HOSTNAME="server.bench"
ARG PORT=8080
ARG SERVER_BASE_DIR="server_base"
ARG DOWNLOAD_DIR="download"
ARG UPLOAD_DIR="upload"
ARG CONFIG_FILE="nginx.conf"

RUN apt install -y nginx openssl

WORKDIR /bench/$SERVER_BASE_DIR

RUN mkdir -p ssl && \
    openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
      -keyout ssl/server.key \
      -out ssl/server.crt \
      -subj "/C=US/ST=Test/L=Test/O=Test/CN=$HOSTNAME"

RUN echo "worker_processes auto;\n\
\n\
events {\n\
  worker_connections 4096;\n\
  multi_accept on;\n\
}\n\
\n\
http {\n\
  sendfile on;\n\
\n\
  server {\n\
    listen        $PORT quic reuseport;\n\
    http3 on;\n\
    listen        $PORT ssl;\n\
    http2 on;\n\
    server_name   $HOSTNAME;\n\
    root          /bench/$SERVER_BASE_DIR/;\n\
    ssl_certificate     /bench/$SERVER_BASE_DIR/ssl/server.crt;\n\
    ssl_certificate_key /bench/$SERVER_BASE_DIR/ssl/server.key;\n\
    ssl_protocols       TLSv1.3;\n\
\n\
    location /$UPLOAD_DIR {\n\
      autoindex               on;\n\
      client_body_temp_path   /bench/$SERVER_BASE_DIR/$UPLOAD_DIR/;\n\
      create_full_put_path    on;\n\
      client_max_body_size    0;\n\
      dav_methods             PUT;\n\
      dav_access              group:rw  all:r;\n\
    }\n\
\n\
    location /$DOWNLOAD_DIR {\n\
    }\n\
  }\n\
}" >>$CONFIG_FILE

ADD entrypoint.sh /bench/$SERVER_BASE_DIR

ENV CONFIG=$CONFIG_FILE \
    DOWNLOAD_DIR=$DOWNLOAD_DIR \
    UPLOAD_DIR=$UPLOAD_DIR
ENTRYPOINT ./entrypoint.sh "$CONFIG" "$DOWNLOAD_DIR" "$UPLOAD_DIR"
