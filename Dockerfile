# NOTE: use Google's "distroless with libgcc1" base image, see:
#       https://github.com/GoogleContainerTools/distroless/blob/6755e21ccd99ddead6edc8106ba03888cbeed41a/cc/README.md
ARG BASE_IMAGE_FINAL_STAGES="gcr.io/distroless/cc:nonroot"

FROM rust:1-bookworm AS builder

WORKDIR /usr/src/lidi
COPY . .
RUN cargo install --path .

FROM ${BASE_IMAGE_FINAL_STAGES} AS send

COPY --from=builder --chown=root:root --chmod=755 /usr/local/cargo/bin/diode-send /usr/local/bin/
ENTRYPOINT ["diode-send"]

FROM ${BASE_IMAGE_FINAL_STAGES} AS receive

COPY --from=builder --chown=root:root --chmod=755 /usr/local/cargo/bin/diode-receive /usr/local/bin/
ENTRYPOINT ["diode-receive"]