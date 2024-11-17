FROM rust:1-bookworm AS builder

WORKDIR /usr/src/lidi
COPY . .
RUN cargo install --path .

# NOTE: use Google's "distroless with glibc" base image, see:
#       https://github.com/GoogleContainerTools/distroless/blob/6755e21ccd99ddead6edc8106ba03888cbeed41a/cc/README.md
FROM gcr.io/distroless/cc:latest AS send

COPY --from=builder /usr/local/cargo/bin/diode-send /usr/local/bin/
ENTRYPOINT ["diode-send"]

FROM gcr.io/distroless/cc:latest AS receive

COPY --from=builder /usr/local/cargo/bin/diode-receive /usr/local/bin/
ENTRYPOINT ["diode-receive"]