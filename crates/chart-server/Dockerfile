# syntax=docker/dockerfile:1
FROM gcr.io/distroless/cc-debian12:nonroot

COPY chart-server-bin /usr/local/bin/chart-server

EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/chart-server"]
