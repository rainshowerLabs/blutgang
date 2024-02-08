# Use Rust official image for the build stage
FROM rust:latest AS build

# Create and set the working directory
WORKDIR /usr/src/app

# Copy the Rust project to the working directory
COPY . .

# Install necessary dependencies for building the Rust project
RUN apt-get update && apt-get install -y libssl-dev pkg-config && \
    # Clean up the apt cache to reduce image size
    apt-get clean && rm -rf /var/lib/apt/lists/*

# Build the Rust project
# If your project uses a custom profile, replace `--release` with `--profile <your-profile>`
RUN RUSTFLAGS='-C target-cpu=native' cargo build --release

# Start a new stage to create a smaller final image
FROM debian:bookworm

# Install runtime dependencies
RUN apt-get update && apt-get install -y openssl ca-certificates && \
    # Clean up the apt cache to reduce image size
    apt-get clean && rm -rf /var/lib/apt/lists/*

# Create a directory for the application
WORKDIR /app

# Copy the built binary from the build stage to the application directory
COPY --from=build /usr/src/app/target/release/blutgang /app/blutgang

# Set the command to run the application
CMD ["./blutgang", "-c", "config.toml"]
