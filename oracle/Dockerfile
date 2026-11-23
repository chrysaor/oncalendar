# Reference implementation for differential tests: the real `systemd-analyze calendar`.
# The base image is pinned by digest and the two packages by version, so a rebuild either gets
# the same systemd and tzdata or fails. When Debian retires these versions the build fails:
# raise the versions here and regenerate tests/fixtures/expected.tsv in the same change.
FROM debian:trixie-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2

RUN apt-get update \
	&& apt-get install -y --no-install-recommends systemd=257.13-1~deb13u1 tzdata=2026f-0+deb13u1 \
	&& rm -rf /var/lib/apt/lists/*

COPY query.pl /usr/local/bin/oracle-query
RUN chmod +x /usr/local/bin/oracle-query

ENTRYPOINT ["/usr/local/bin/oracle-query"]
