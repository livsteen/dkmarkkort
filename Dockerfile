# To images fra samme build: serveren (standard) og pipelinen, der bygger
# markkort.gpkg, marker.mbtiles, overblik.mbtiles og sprøjtningens filer i
# /data. De deler data gennem et volume, se compose.yaml.
#
#   docker build -t dkmarkkort .
#   docker build --target pipeline -t dkmarkkort-pipeline .
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
RUN cargo build --release -p dkmarkkort -p dkmarkkort-pipeline \
    && topcoat asset bundle --release -p dkmarkkort

# Pipelinens input, der ligger i repoet.
COPY data/dagi-landsdele.geojson data/afgroedekoder-*.csv ./data/

# Pipelinens version er et fingeraftryk af alt, der afgør hvad den bygger.
# Ændrer det sig, bygger pipeline-containeren data forfra.
RUN find crates/pipeline crates/core data Cargo.lock -type f | sort \
    | xargs sha256sum | sha256sum | cut -d' ' -f1 > pipelineversion

# ---- Pipeline ---------------------------------------------------------------
FROM debian:trixie-slim AS pipeline

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates gdal-bin tippecanoe \
    && rm -rf /var/lib/apt/lists/*

# /data ejes af brugeren, så et nyt volume arver ejerskabet og kan skrives.
RUN useradd --system --uid 10001 --no-create-home markkort \
    && mkdir /data && chown markkort /data

COPY --from=byg /src/target/release/dkmarkkort-pipeline /app/dkmarkkort-pipeline
COPY --from=byg /src/pipelineversion /app/pipelineversion
COPY --from=byg /src/data /app/input
COPY deploy/pipeline.sh /app/pipeline.sh

ENV MARKKORT_DATA=/data

USER markkort
VOLUME /data

CMD ["/app/pipeline.sh"]

# ---- Kør --------------------------------------------------------------------
FROM debian:trixie-slim

# Samme bruger og ejerskab som i pipelinen, uanset hvilken container der
# mounter volumet først.
RUN useradd --system --uid 10001 --no-create-home markkort \
    && mkdir /data && chown markkort /data

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
