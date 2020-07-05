#!/usr/bin/env python

import logging
import hello_world

FORMAT = '%(levelname)s %(name)s %(asctime)-15s %(filename)s:%(lineno)d %(message)s'
logging.basicConfig(format=FORMAT)
logging.getLogger().setLevel(logging.INFO)
logging.info("Test 1")
hello_world.log_hello()
