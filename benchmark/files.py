"""显式文件契约和不可覆盖的目录提交。金额保持十进制字符串。"""

import hashlib
import json
import re
import shutil
import tempfile
from contextlib import contextmanager
from datetime import date, datetime
from decimal import Decimal
from pathlib import Path


class Invalid(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise Invalid(message)


def day(value):
    require(isinstance(value, str), "日期必须是 YYYY-MM-DD 字符串")
    parsed = date.fromisoformat(value)
    require(parsed.isoformat() == value, f"日期格式错误: {value}")
    return parsed


def instant(value):
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    require(parsed.tzinfo is not None, "时间必须带时区")
    return parsed


def number(value, positive=False):
    require(isinstance(value, str), "价格、金额、比率必须使用十进制字符串")
    parsed = Decimal(value)
    require(parsed.is_finite(), "数值必须有限")
    require(parsed > 0 if positive else parsed >= 0, "数值范围错误")
    return parsed


def symbol(value):
    require(isinstance(value, str) and re.fullmatch(r"\d{6}\.(SH|SZ)", value),
            f"证券代码错误: {value}")
    return value


def source(value):
    require(isinstance(value, str) and value.startswith("https://"), "来源必须是 HTTPS URL")


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def rows(path):
    if not Path(path).exists():
        return []
    return [json.loads(line) for line in Path(path).read_text(encoding="utf-8-sig").splitlines()
            if line.strip()]


def write_json(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n",
                          encoding="utf-8")


def write_rows(path, values):
    with Path(path).open("w", encoding="utf-8", newline="\n") as handle:
        for value in values:
            handle.write(json.dumps(value, ensure_ascii=False, sort_keys=True, allow_nan=False) + "\n")


def digest(path):
    with Path(path).open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def local_file(root, name):
    path = (Path(root) / name).resolve()
    require(path.is_relative_to(Path(root).resolve()), "文件路径越出数据目录")
    require(path.is_file(), f"文件不存在: {name}")
    return path


@contextmanager
def new_directory(destination):
    destination = Path(destination).resolve()
    require(not destination.exists(), f"目标已存在，请使用新版本目录: {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix=".benchmark-", dir=destination.parent))
    try:
        yield stage
        require(not destination.exists(), "目标被其他进程创建")
        stage.rename(destination)
    finally:
        if stage.exists():
            require(stage.resolve().parent == destination.parent and stage.name.startswith(".benchmark-"),
                    "临时目录超出目标父目录，拒绝清理")
            shutil.rmtree(stage)
