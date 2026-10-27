"""从通过校验的输入原子构建版本化发布目录；不自动上传。"""

import shutil
from .dataset import Dataset, TABLES
from .files import digest, local_file, new_directory, require, write_json, write_rows


def build(root, destination):
    dataset = Dataset(root)
    validation = dataset.validate()
    require(validation["valid"], "数据校验失败: " + "; ".join(validation["errors"]))
    samples = list(dataset.samples())
    labels = [label for sample in samples for label in dataset.labels(sample)]
    with new_directory(destination) as stage:
        write_json(stage / "config.json", dataset.config)
        write_json(stage / "validation.json", validation)
        for table in TABLES:
            table_rows = dataset.tables[table]
            if table == "reports":
                table_rows = [{**row, "document_path": f"documents/{row['sha256']}.pdf"} for row in table_rows]
            write_rows(stage / f"{table}.jsonl", table_rows)
        documents = stage / "documents"
        documents.mkdir()
        for report in dataset.tables["reports"]:
            shutil.copyfile(local_file(root, report["document_path"]), documents / f"{report['sha256']}.pdf")
        (stage / "inputs").mkdir()
        (stage / "labels").mkdir()
        counts = {}
        for split, (start, end) in dataset.config["splits"].items():
            inputs = [s for s in samples if start <= s["decision_date"] <= end]
            # 文档引用同步为发布目录中的内容寻址路径。
            for sample in inputs:
                for item in sample["stocks"] + sample["etfs"]:
                    for report in item["reports"]:
                        report["document_path"] = f"documents/{report['sha256']}.pdf"
            outcomes = [label for label in labels if label["split"] == split]
            write_rows(stage / "inputs" / f"{split}.jsonl", inputs)
            write_rows(stage / "labels" / f"{split}.jsonl", outcomes)
            counts[split] = {"samples": len(inputs), "labels": len(outcomes),
                             "unavailable_labels": sum(r["status"] != "observed" for r in outcomes)}
        summary = {"version": dataset.config["version"], "start": dataset.config["start"],
                   "end": dataset.config["end"], "unique_stocks": len(dataset.selected_symbols()),
                   "etfs": dataset.config["etfs"], "tables": validation["counts"], "splits": counts,
                   "price_basis": "原始 OHLC + 当日复权因子；收益为供应商复权收益代理",
                   "redistribution_status": "not_reviewed", "published": False}
        write_json(stage / "summary.json", summary)
        configs = []
        for name in ("inputs", "labels"):
            configs += [f"- config_name: {name}", "  data_files:"]
            for split in dataset.config["splits"]:
                configs += [f"  - split: {split}", f"    path: {name}/{split}.jsonl"]
        (stage / "README.md").write_text(
            "---\nlanguage:\n- zh\nconfigs:\n" + "\n".join(configs) + "\n---\n\n"
            "# 中国红利 top-20 历史研究 benchmark\n\n"
            f"版本：{dataset.config['version']}。区间：{dataset.config['start']} 至 {dataset.config['end']}。\n\n"
            "每年从当时中证红利候选池按上一年末总市值选前 20。保留退出样本和全部财报修订版本。\n\n"
            "`inputs` 为季度末 18:00（北京时间）可知的模型输入；`labels` 为从下一交易日开盘开始的"
            "21/63/252 交易日复权收益代理，标签不跨 split。完整行情与 PDF 是研究归档，"
            "不能作为未截断的模型输入。财报指标为原报告口径，不自动把累计数当单季值。\n\n"
            "研究用途：预测横截面排序、比较宽指和等权基线。没有可成交组合回测、收益承诺或自动交易。"
            "供应商复权不等于税后现金分红再投资。LLM 预训练记忆可能污染历史测试。\n\n"
            "来源见各记录 source_uri；文件完整性见 manifest.json。数据再分发授权尚未审查，"
            "未指定开放许可证。本目录仅为本地待发布包。\n", encoding="utf-8")
        files = {p.relative_to(stage).as_posix(): digest(p) for p in sorted(stage.rglob("*")) if p.is_file()}
        write_json(stage / "manifest.json", {"schema_version": 1, "files": files})
    return summary


def verify_release(root):
    from .files import read_json

    manifest = read_json(root / "manifest.json")
    actual = {p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file()}
    require(actual == set(manifest["files"]) | {"manifest.json"}, "发布包文件集合与 manifest 不一致")
    for name, expected in manifest["files"].items():
        require(digest(local_file(root, name)) == expected, f"发布包文件被改动: {name}")
    return {"valid": True, "files": len(actual)}
