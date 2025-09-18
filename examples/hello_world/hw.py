#!/usr/bin/env python

import logging
import hello_world


class ExtraInfoFormatter(logging.Formatter):
    def format(self, record):
        record_dict = record.__dict__.copy()
        standard_keys = logging.makeLogRecord({}).__dict__.keys()
        extras = {k: v for k, v in record_dict.items()
                  if k not in standard_keys}
        extras_string = f" {extras}" if extras else ""
        formatted_message = super().format(record)
        return f"{formatted_message}{extras_string}"


logger = logging.getLogger('hello_world')
handler = logging.StreamHandler()
handler.setFormatter(ExtraInfoFormatter("%(levelname)s:%(message)s"))
logger.addHandler(handler)
logger.setLevel(logging.INFO)

logger.info("Hello, world!", extra={"foo": 42, "bar": "baz"})

hello_world.log_hello()
