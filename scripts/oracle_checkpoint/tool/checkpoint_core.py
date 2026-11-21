"""Strict evidence I/O. A validation result never grants acceptance."""
from pathlib import Path
import hashlib
import json


class EvidenceError(ValueError):
    pass


def strict_load(text):
    def pairs(items):
        value = {}
        for key, child in items:
            if key in value:
                raise EvidenceError('duplicate JSON key: ' + key)
            value[key] = child
        return value
    def constant(value):
        raise EvidenceError('nonfinite JSON value: ' + value)
    return json.loads(text, object_pairs_hook=pairs, parse_constant=constant)


def typed(value):
    """CASE + JSON type exact; True != 1 and 1 != 1.0."""
    if isinstance(value, dict):
        return ('object', tuple(sorted((key, typed(child)) for key, child in value.items())))
    if isinstance(value, list):
        return ('array', tuple(map(typed, value)))
    return (type(value).__name__, value)


def require(condition, message):
    if not condition:
        raise EvidenceError(message)


def keys(value, required, optional=()):
    require(isinstance(value, dict), 'expected an object')
    require(set(required) <= value.keys(), 'missing fields: ' + str(sorted(set(required)-value.keys())))
    require(value.keys() <= set(required) | set(optional), 'unknown fields: ' + str(sorted(value.keys()-set(required)-set(optional))))


def count(value, label, positive=False):
    require(type(value) is int and value >= (1 if positive else 0), 'invalid count ' + label)
    return value


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest().upper()


class Evidence:
    def __init__(self, repo):
        self.repo = Path(repo).resolve()
        self.seen = {}

    def path(self, value):
        require(isinstance(value, str) and bool(value), 'empty evidence path')
        path = (self.repo/value).resolve()
        require(path.is_relative_to(self.repo), 'evidence path escapes repository: ' + value)
        require(path.is_file(), 'missing evidence: ' + value)
        return path

    def ref(self, value):
        keys(value, ['path', 'sha256'])
        require(isinstance(value['sha256'], str) and len(value['sha256']) == 64
                and all(char in '0123456789ABCDEF' for char in value['sha256']), 'invalid SHA256')
        path = self.path(value['path'])
        require(digest(path) == value['sha256'], 'hash mismatch: ' + value['path'])
        self.seen[path] = value['sha256']
        return path

    def json(self, ref):
        return strict_load(self.ref(ref).read_text(encoding='utf-8-sig'))

    def text(self, ref, log=False):
        raw=self.ref(ref).read_bytes()
        if raw.startswith((b'\xff\xfe',b'\xfe\xff')):
            require(log, 'Oracle/source JSON must remain UTF-8; UTF-16 is allowed only for BOM-marked transcripts')
            return raw.decode('utf-16')
        return raw.decode('utf-8-sig')

    def jsonl(self, ref, mixed=False):
        result = []
        for line in self.text(ref,log=mixed).splitlines():
            if not line.strip() or (mixed and not line.startswith('{')):
                continue
            value=strict_load(line)
            require(isinstance(value,dict),'JSONL result row must be an object')
            result.append(value)
        return result

    def hash_map(self, mapping):
        require(isinstance(mapping, dict) and bool(mapping), 'empty source/fixture hash map')
        for path, expected in mapping.items():
            self.ref({'path':path, 'sha256':expected})
        return len(mapping)

    def final_check(self):
        for path, expected in self.seen.items():
            require(digest(path) == expected, 'evidence changed during validation: ' + str(path))


def fresh_output(repo, requested):
    path = Path(requested)
    path = (Path(repo)/path).resolve()
    temporary=(Path(repo)/'.tmp').resolve()
    require(temporary.is_relative_to(Path(repo).resolve()) and path.is_relative_to(temporary), 'output must stay under repository/.tmp')
    require(not path.exists(), 'output must be new; never overwrite a checkpoint or source')
    return path
