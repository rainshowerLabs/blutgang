# Use an official Rust image as the base image
FROM rust:latest

# Set the working directory
WORKDIR /app

# Copy the project files into the container
COPY / /app

# Install libssl-dev
RUN apt-get update && apt-get install -y libssl-dev

# Build and run the Rust project
CMD ["cargo", "run", "--profile", "maxperf", "--", "-c", "config.toml"]
