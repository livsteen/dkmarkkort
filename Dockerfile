# Serveren alene. Data — markkort.gpkg og marker.mbtiles — bygges uden for
# containeren med dkmarkkort-pipeline og mountes på /data:
#
#   docker build -t dkmarkkort .
#   docker run --rm -p 3000:3000 -v "$PWD/data:/data:ro" dkmarkkort

# ---- Byg --------------------------------------------------------------------
FROM rust:1.98-trixie AS byg

# topcoat-CLI'en samler assets (OpenLayers, markkort.js og det genererede
# Tailwind-stylesheet) i den mappe serveren læser dem fra.
RUN cargo install topcoat-cli --version 0.9.0 --locked

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# build.rs henter Tailwind CLI til platformen den bygges på.
RUN cargo build --release -p dkmarkkort \
    && topcoat asset bundle --release -p dkmarkkort

# ---- Kør --------------------------------------------------------------------
FROM debian:trixie-slim

RUN useradd --system --uid 10001 --no-create-home markkort

# Asset-bundtet skal ligge ved siden af programmet, fra samme build.
COPY --from=byg /src/target/release/dkmarkkort /app/dkmarkkort
COPY --from=byg /src/target/release/assets /app/assets

ENV HOST=0.0.0.0 \
    PORT=3000 \
    MARKKORT_DATA=/data

USER markkort
EXPOSE 3000
VOLUME /data

CMD ["/app/dkmarkkort"]
