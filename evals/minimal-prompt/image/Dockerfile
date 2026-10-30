# Solver and grader image: Python 3.12, pytest, and pysnappea from this checkout,
# installed with pip metadata like a real wheel (`pip show pysnappea` works).
# The snappea binary is mounted at /opt/bin/snappea at run time.
FROM python:3.12-slim
COPY wheels /tmp/wheels
RUN pip install --no-cache-dir --no-index /tmp/wheels/*.whl && useradd -m -u 1000 dev && rm -rf /tmp/wheels
ENV PATH=/opt/bin:/usr/local/bin:/usr/bin:/bin SNAPPEA_BIN=/opt/bin/snappea PYTHONDONTWRITEBYTECODE=1
USER dev
WORKDIR /work
