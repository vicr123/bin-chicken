FROM rust:alpine AS build
LABEL authors="victor"

WORKDIR /usr/src/bin-chicken
COPY . .

RUN cargo build --release

FROM alpine
COPY --from=build /usr/src/bin-chicken/target/release/bin-chicken /usr/bin/bin-chicken

ENTRYPOINT ["/usr/bin/bin-chicken", "-c", "/etc/bin-chicken.yaml"]