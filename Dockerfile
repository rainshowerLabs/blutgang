# Use an official Rust image as the base image
FROM rust:latest

# Set the working directory
WORKDIR /app

# Copy the project files into the container
COPY / /app

# Remove all existing remotes
RUN git remote | xargs -L1 git remote remove

# Add a new HTTPS GitHub remote
RUN git remote add origin https://github.com/rainshowerLabs/blutgang.git

# Update the repository (pull the latest changes)
RUN git pull origin master

RUN pwd

RUN ls -la

# Build and run the Rust project
CMD ["cargo", "run", "--profile", "maxperf", "--", "-c", "config.toml"]
