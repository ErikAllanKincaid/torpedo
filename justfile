target := "x86_64-unknown-linux-gnu"
binary := "pitopi"
user := "root"

build:
    cargo -q build

release:
    cargo -q build --release

cross:
    cross -q build --release --target {{target}}

deploy ip:
    cross -q build --release --target {{target}}
    rsync -az --progress target/{{target}}/release/{{binary}} {{user}}@{{ip}}:/tmp/
    ssh {{user}}@{{ip}} "install -m 755 /tmp/{{binary}} /usr/local/bin/{{binary}}"
    @echo "Deployed to {{ip}}"

check:
    cargo -q check

run *args:
    sudo cargo -q run -- {{args}}
