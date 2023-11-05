FROM rust:1.73
LABEL authors="nicholasball"
EXPOSE 8000
ENV PORT 8000

WORKDIR /usr/src/myapp
COPY . .

RUN apt update

RUN apt install llvm-dev libclang-dev clang libopencv-dev -y

RUN rustup default nightly

RUN cargo install --path .

CMD ["Tech120FinalDemo"]