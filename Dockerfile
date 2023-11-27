FROM rust:1.73
LABEL authors="nicholasball"
EXPOSE 8000
ENV PORT 8000

RUN apt update

RUN apt install llvm-dev libclang-dev clang cmake wget unzip -y

RUN wget -O opencv.zip https://github.com/opencv/opencv/archive/4.5.4.zip
RUN unzip opencv.zip
RUN mkdir -p build && cd build
RUN ls
RUN cmake opencv-4.5.4
RUN cmake --build .

RUN make install
RUN sh -c 'echo "/usr/local/lib" >> /etc/ld.so.conf.d/opencv.conf'
RUN ldconfig

RUN rustup default nightly

WORKDIR /usr/src/myapp
COPY . .

RUN cargo install --path .

CMD ["Tech120FinalDemo"]