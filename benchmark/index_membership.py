"""中证官方当前完整成分采集；只用于扩展行情归档，不冒充历史成分。"""

import hashlib
from datetime import datetime

from .collect import retry
from .files import day, digest, read_json, require, symbol, write_json
from .public_sources import SHANGHAI, download
from .xls import sheet_rows

INDICES = {"930050": "中证A50", "000016": "上证50"}
SOURCE = "https://oss-ch.csindex.com.cn/static/html/csindex/public/uploads/file/autofile/cons/{}cons.xls"
EXCHANGES = {"上海证券交易所": "SH", "深圳证券交易所": "SZ"}


def code6(value):
    if isinstance(value, (int, float)):
        require(int(value) == value and 0 <= value < 1000000, "成分文件代码不是六位整数")
        value = f"{int(value):06d}"
    require(isinstance(value, str) and len(value) == 6 and value.isdigit(), "成分文件代码格式错误")
    return value


def parse_membership(content, index):
    require(index in INDICES, f"不支持的完整成分指数: {index}")
    table = sheet_rows(content)
    require(table and len(table[0]) >= 9 and "Constituent Code" in table[0][4]
            and "Exchange" in table[0][7], "中证成分文件表头发生变化")
    dates, members = set(), {}
    for row in table[1:]:
        require(len(row) >= 9 and code6(row[1]) == index and row[2] == INDICES[index], "成分文件指数身份不符")
        date = str(int(row[0])) if isinstance(row[0], (int, float)) else row[0]
        require(isinstance(date, str) and len(date) == 8 and date.isdigit(), "成分文件日期格式错误")
        dates.add(day(f"{date[:4]}-{date[4:6]}-{date[6:]}").isoformat())
        require(row[7] in EXCHANGES, f"不支持的成分交易所: {row[7]}")
        code = symbol(f"{code6(row[4])}.{EXCHANGES[row[7]]}")
        require(code not in members, f"成分文件证券重复: {code}")
        require(isinstance(row[5], str) and row[5].strip(), f"成分证券名称为空: {code}")
        members[code] = {"symbol": code, "name": row[5]}
    require(len(dates) == 1 and len(members) == 50, f"{INDICES[index]} 必须是同一日期的完整50只成分")
    return dates.pop(), [members[code] for code in sorted(members)]


def collect_memberships(root, observed_on=None):
    observed_on = observed_on or datetime.now(SHANGHAI).date().isoformat()
    day(observed_on)
    result = {}
    for index, name in INDICES.items():
        uri = SOURCE.format(index)
        content = retry(lambda: download(uri, referer="https://www.csindex.com.cn/"))
        source_date, members = parse_membership(content, index)
        require(source_date <= observed_on, "成分文件日期晚于获取日期")
        raw_dir = root / "index-membership" / index
        raw_dir.mkdir(parents=True, exist_ok=True)
        # 原件与解释以内容哈希标识；同日修正保留两个版本，重复采集不制造新版本。
        sha = hashlib.sha256(content).hexdigest()
        stem = f"{observed_on}-{sha[:16]}"
        raw = raw_dir / f"{stem}.xls"
        raw.write_bytes(content)
        snapshot = {"schema_version": 1, "index": index, "name": name, "source_date": source_date,
                    "observed_on": observed_on, "membership_kind": "current_official_snapshot",
                    "historical_membership_verified": False, "source_uri": uri, "sha256": sha,
                    "raw_path": raw.relative_to(root).as_posix(), "members": members}
        write_json(raw_dir / f"{stem}.json", snapshot)
        result[index] = {"name": name, "source_date": source_date, "observed_on": observed_on, "members": len(members)}
    return result


def membership_selection(root):
    codes, latest = set(), {}
    for path in sorted((root / "index-membership").glob("*/*.json")):
        snapshot = read_json(path)
        index = snapshot["index"]
        require(index in INDICES and snapshot["schema_version"] == 1
                and snapshot["name"] == INDICES[index]
                and snapshot["source_uri"] == SOURCE.format(index)
                and snapshot["historical_membership_verified"] is False
                and snapshot["membership_kind"] == "current_official_snapshot",
                "成分快照身份错误")
        require(day(snapshot["source_date"]) <= day(snapshot["observed_on"]), "成分快照日期顺序错误")
        raw = root / snapshot["raw_path"]
        require(raw.resolve().is_relative_to(root.resolve()) and raw.is_file(), "成分原件路径错误")
        require(digest(raw) == snapshot["sha256"], "成分原件哈希错误")
        source_date, members = parse_membership(raw.read_bytes(), index)
        require(source_date == snapshot["source_date"] and members == snapshot["members"], "成分解释与原件不符")
        codes.update(row["symbol"] for row in members)
        # 同一获取日的多个修订不猜哪个生效；全部保留并参与采集并集。
        latest.setdefault(index, []).append(snapshot)
    summaries = {}
    for index, values in latest.items():
        last_day = max(v["observed_on"] for v in values)
        current = [v for v in values if v["observed_on"] == last_day]
        summaries[index] = {"name": INDICES[index], "observed_on": last_day,
                            "source_dates": sorted({v["source_date"] for v in current}),
                            "snapshots": len(values), "members_per_snapshot": 50,
                            "current_members_union": len({r["symbol"] for v in current for r in v["members"]})}
    return sorted(codes), summaries
