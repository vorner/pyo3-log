#!/usr/bin/env python

import logging
import hello_world

FORMAT = '%(levelname)s %(name)s %(asctime)-15s %(filename)s:%(lineno)d %(message)s'
logger = logging.getLogger('test')
logger.setLevel(logging.INFO)
handler = logging.StreamHandler()
handler.setFormatter(logging.Formatter(FORMAT))
logger.addHandler(handler)

hello_world.enable_logging(prefix="test")
hello_world.log_hello()
