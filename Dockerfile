# Use an official Rust image as the base image
FROM rust:latest

# Set the working directory
WORKDIR /app

# Copy the project files into the container
COPY / /app

# Pull the latest changes from the master branch
RUN git pull origin master

# Build and run the Rust project
CMD ["cargo", "run", "--", "-c", "config.toml"]
