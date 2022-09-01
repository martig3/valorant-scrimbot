FROM rust:1.60 as build

# create a new empty shell project
RUN cargo new --bin valorant-scrimbot
WORKDIR /valorant-scrimbot

COPY . .

RUN cargo build --release

# our final base
FROM rust:1.60-slim-buster

# copy the build artifact from the build stage
COPY --from=build /valorant-scrimbot/target/release/valorant-scrimbot .

# set the startup command to run your binary
CMD ["./valorant-scrimbot"]
