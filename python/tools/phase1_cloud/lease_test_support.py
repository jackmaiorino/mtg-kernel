"""Bounded fakes for the lease-guard tests. No provider, network or subprocess call.

Every guard retry loop catches Exception, so a fake that runs out of scripted
answers must end the test with Stop (a BaseException) instead of looping. The
fake clock advances only when the code under test sleeps and raises Stop after
a hard sleep cap; the fake provider raises Stop after a hard call cap and keeps
no unbounded history. Use no_network() around every guard call.
"""
from __future__ import annotations

import contextlib
import socket
import subprocess
import threading
import urllib.request
from unittest import mock

MAX_SLEEPS = 400
MAX_CALLS = 2000


class Stop(BaseException):
    """Ends a test loop; escapes every `except Exception` in the guards."""


class FakeClock:
    def __init__(self, t=1000.0, max_sleeps=MAX_SLEEPS):
        self.t = float(t); self.sleeps = 0; self.max_sleeps = max_sleeps

    def __call__(self):
        return self.t

    def sleep(self, seconds):
        self.sleeps += 1
        if self.sleeps > self.max_sleeps:
            raise Stop('fake clock sleep cap reached')
        self.t += max(0.0, float(seconds))

    def advance(self, seconds):
        self.t += seconds


@contextlib.contextmanager
def guard_time(module, clock):
    """Patch the module's time.time and time.sleep (the shared time module) with the fake clock."""
    with mock.patch.object(module.time, 'time', clock), mock.patch.object(module.time, 'sleep', clock.sleep):
        yield clock


@contextlib.contextmanager
def no_network():
    def refuse(*args, **kwargs):
        raise Stop('network or subprocess call attempted in an offline test')
    with mock.patch.object(urllib.request, 'urlopen', refuse), \
            mock.patch.object(socket.socket, 'connect', refuse), \
            mock.patch.object(subprocess, 'run', refuse), \
            mock.patch.object(subprocess, 'Popen', refuse):
        yield


FUNDS_OK = {'balance_usd': 164.0, 'account_spend_usd_hour': 0.05, 'under_balance': False}


class World:
    """Fake provider state: Pods by ID, GET/DELETE per ID, a census and an account read."""

    def __init__(self, pods=(), max_calls=MAX_CALLS):
        self.pods = {p['id']: dict(p) for p in pods}
        self.calls = 0; self.max_calls = max_calls
        self.log = []                 # (method, pod_id), capped with the call count
        self.deletes = []
        self.delete_failures = 0      # the next N DELETEs fail; -1 = always
        self.get_failures = 0         # the next N GETs fail; -1 = always
        self.list_fails = False
        self.get_script = None        # optional iterator of 'absent' / 'present' overriding GET
        self.funds_value = dict(FUNDS_OK)
        self.funds_script = None      # optional iterator of account reads (an Exception instance raises)
        self.funds_calls = 0
        self.lock = threading.Lock()

    def _count(self, entry):
        self.calls += 1
        if self.calls > self.max_calls:
            raise Stop('fake provider call cap reached')
        self.log.append(entry)

    def list(self, key):
        with self.lock:
            self._count(('LIST', None))
            if self.list_fails:
                raise RuntimeError('census HTTP 503')
            return [dict(p) for p in self.pods.values()]

    def provider(self, key, pod_id):
        return FakeApi(self, pod_id)

    def methods(self):
        return [method for method, _ in self.log if method != 'LIST']


class FakeApi:
    def __init__(self, world, pod_id):
        self.world = world; self.pod_id = pod_id

    def call(self, method):
        world = self.world
        with world.lock:
            world._count((method, self.pod_id))
            if method == 'GET':
                if world.get_failures:
                    world.get_failures -= world.get_failures > 0
                    raise RuntimeError('provider HTTP 503')
                if world.get_script is not None:
                    step = next(world.get_script, 'absent')   # 'absent', 'present' or a Pod dict
                    if isinstance(step, dict):
                        return dict(step)
                    return None if step == 'absent' else dict(world.pods.get(self.pod_id) or {'id': self.pod_id})
                return dict(world.pods[self.pod_id]) if self.pod_id in world.pods else None
            if method == 'DELETE':
                world.deletes.append(self.pod_id)
                if world.delete_failures:
                    world.delete_failures -= world.delete_failures > 0
                    raise RuntimeError('provider HTTP 500')
                world.pods.pop(self.pod_id, None)
                return {}
            raise ValueError('guard method rejected')

    def funds(self):
        world = self.world
        with world.lock:
            world._count(('FUNDS', None))
            world.funds_calls += 1
            value = world.funds_value
            if world.funds_script is not None:
                value = next(world.funds_script, value)
            if isinstance(value, Exception):
                raise value
            return dict(value)
