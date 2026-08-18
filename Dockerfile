FROM golang:1.26.6-alpine3.24@sha256:3889b425f035be855a72fb4755265311293b6d414521f0a519d819df32222d83 AS build
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN apk add --no-cache tzdata && \
    CGO_ENABLED=0 go build -trimpath -ldflags="-s -w" -o /out/myurls .

FROM scratch
WORKDIR /app
COPY --from=build --chown=65532:65532 /out/myurls /app/myurls
COPY --chown=65532:65532 public /app/public
COPY --from=build /usr/share/zoneinfo/Asia/Shanghai /usr/share/zoneinfo/Asia/Shanghai
COPY --from=build /usr/share/zoneinfo/Asia/Shanghai /etc/localtime
ENV TZ=Asia/Shanghai
USER 65532:65532
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 CMD ["/app/myurls", "-healthcheck"]
ENTRYPOINT ["/app/myurls"]
