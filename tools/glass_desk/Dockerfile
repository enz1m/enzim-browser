# KWin (Plasma's compositor) for tools/glass_desk/kwin: a desk on a virtual output.
FROM ubuntu:26.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      kwin-wayland kwin-common kwin-data dbus dbus-user-session libgl1-mesa-dri libegl1 libgles2 \
      python3 python3-dbus python3-pil python3-gi fonts-dejavu-core \
    && rm -rf /var/lib/apt/lists/*
