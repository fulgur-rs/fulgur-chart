# syntax=docker/dockerfile:1

# distroless にはシェルが無いため、書き込み可能な shortlink 保存 dir を
# シェルのあるステージで用意し、nonroot(uid/gid 65532) 所有にしておく。
FROM busybox:1.37 AS prep
RUN mkdir -p /data && chown 65532:65532 /data

FROM gcr.io/distroless/cc-debian12:nonroot

COPY chart-server-bin /usr/local/bin/chart-server

# shortlink を永続化する既定 dir。nonroot 所有の空 dir をイメージに焼き込む。
# distroless の cwd は `/`（nonroot は書けない）ため、明示的に書き込み可能な
# `/data` を既定にして起動時の create_dir_all が EACCES で panic しないようにする。
# named volume を `/data` にマウントすると Docker がこの所有権を継承するので、
# redeploy をまたいでも nonroot が書き込め、リンクが永続化される（未マウント時は
# イメージ内 `/data` に書くため起動はするが ephemeral）。
COPY --from=prep --chown=65532:65532 /data /data
ENV FULGUR_SHORTLINK_DIR=/data

EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/chart-server"]
