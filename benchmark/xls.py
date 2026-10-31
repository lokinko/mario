"""只读解析旧版 Excel（OLE2 + BIFF8）第一个工作表的文本与数值单元格。"""

import struct

from .files import require

END_OF_CHAIN = 0xFFFFFFFE
BOF, EOF, CONTINUE, SST = 0x0809, 0x000A, 0x003C, 0x00FC
LABELSST, LABEL, NUMBER, RK, MULRK = 0x00FD, 0x0204, 0x0203, 0x027E, 0x00BD
WORKSHEET = 0x0010


def _workbook_stream(data):
    require(data[:8] == bytes.fromhex("d0cf11e0a1b11ae1"), "不是 OLE2 Excel 文件")
    sector = 1 << struct.unpack_from("<H", data, 30)[0]
    fat_count, directory_start = struct.unpack_from("<II", data, 44)
    cutoff = struct.unpack_from("<I", data, 56)[0]
    difat_start, difat_count = struct.unpack_from("<II", data, 68)

    def offset(index):
        return (index + 1) * sector

    fat_sectors = list(struct.unpack_from("<109I", data, 76))
    per_sector = sector // 4
    current = difat_start
    for _ in range(difat_count):
        entries = struct.unpack_from(f"<{per_sector}I", data, offset(current))
        fat_sectors += entries[:-1]
        current = entries[-1]
    fat = []
    for index in fat_sectors[:fat_count]:
        fat += struct.unpack_from(f"<{per_sector}I", data, offset(index))

    def chain(start):
        output, current, seen = bytearray(), start, set()
        while current != END_OF_CHAIN:
            require(current < len(fat) and current not in seen, "OLE2 扇区链损坏")
            seen.add(current)
            output += data[offset(current):offset(current) + sector]
            current = fat[current]
        return bytes(output)

    directory = chain(directory_start)
    for position in range(0, len(directory), 128):
        entry = directory[position:position + 128]
        name_length = struct.unpack_from("<H", entry, 64)[0]
        name = entry[:max(name_length - 2, 0)].decode("utf-16-le")
        if name in ("Workbook", "Book") and entry[66] == 2:
            start, size = struct.unpack_from("<II", entry, 116)
            require(size >= cutoff, "不支持存放在迷你流中的小型工作簿")
            return chain(start)[:size]
    raise ValueError("Excel 文件缺少 Workbook 流")


def _records(stream):
    position = 0
    while position + 4 <= len(stream):
        kind, length = struct.unpack_from("<HH", stream, position)
        yield kind, stream[position + 4:position + 4 + length]
        position += 4 + length


class _Segments:
    """SST 字符串可跨 CONTINUE 记录；跨界续读的字符段前有新的编码标志字节。"""

    def __init__(self, chunks):
        self.chunks, self.index, self.position = chunks, 0, 0

    def _advance(self):
        if self.position >= len(self.chunks[self.index]):
            self.index += 1
            self.position = 0
            require(self.index < len(self.chunks), "SST 记录被截断")

    def take(self, size):
        output = bytearray()
        while size:
            self._advance()
            chunk = self.chunks[self.index]
            part = chunk[self.position:self.position + size]
            output += part
            self.position += len(part)
            size -= len(part)
        return bytes(output)

    def characters(self, count, wide):
        output = []
        while count:
            if self.position >= len(self.chunks[self.index]):
                self._advance()
                wide = bool(self.take(1)[0] & 1)
            width = 2 if wide else 1
            available = (len(self.chunks[self.index]) - self.position) // width
            part = min(count, available)
            raw = self.take(part * width)
            output.append(raw.decode("utf-16-le" if wide else "latin-1"))
            count -= part
        return "".join(output)


def _unicode_string(segments):
    count = struct.unpack("<H", segments.take(2))[0]
    flags = segments.take(1)[0]
    runs = struct.unpack("<H", segments.take(2))[0] if flags & 8 else 0
    extension = struct.unpack("<I", segments.take(4))[0] if flags & 4 else 0
    text = segments.characters(count, bool(flags & 1))
    segments.take(4 * runs + extension)
    return text


def _rk(value):
    number = (struct.unpack("<d", struct.pack("<Q", (value & 0xFFFFFFFC) << 32))[0]
              if not value & 2 else float(value >> 2 if value < 1 << 31 else (value >> 2) - (1 << 30)))
    return number / 100 if value & 1 else number


def first_sheet(data):
    """返回 {(行, 列): 值}；文本为 str，数值为 float。日期仍是 Excel 序列号。"""
    records = list(_records(_workbook_stream(data)))
    strings, cells, in_sheet = [], {}, False
    for index, (kind, body) in enumerate(records):
        if kind == SST:
            chunks = [body[8:]]
            for follow, extra in records[index + 1:]:
                if follow != CONTINUE:
                    break
                chunks.append(extra)
            segments = _Segments(chunks)
            strings = [_unicode_string(segments) for _ in range(struct.unpack_from("<I", body, 4)[0])]
        elif kind == BOF and struct.unpack_from("<H", body, 2)[0] == WORKSHEET:
            in_sheet = True
        elif in_sheet and kind == EOF:
            break
        elif in_sheet and kind == LABELSST:
            row, column, _, position = struct.unpack_from("<HHHI", body)
            cells[row, column] = strings[position]
        elif in_sheet and kind == LABEL:
            row, column = struct.unpack_from("<HH", body)
            cells[row, column] = _unicode_string(_Segments([body[6:]]))
        elif in_sheet and kind == NUMBER:
            row, column, _, value = struct.unpack_from("<HHHd", body)
            cells[row, column] = value
        elif in_sheet and kind == RK:
            row, column, _, value = struct.unpack_from("<HHHI", body)
            cells[row, column] = _rk(value)
        elif in_sheet and kind == MULRK:
            row, first = struct.unpack_from("<HH", body)
            for offset in range(0, (len(body) - 6) // 6):
                value = struct.unpack_from("<I", body, 4 + offset * 6 + 2)[0]
                cells[row, first + offset] = _rk(value)
    require(in_sheet, "Excel 文件没有工作表")
    return cells


def sheet_rows(data):
    cells = first_sheet(data)
    if not cells:
        return []
    height = max(row for row, _ in cells) + 1
    width = max(column for _, column in cells) + 1
    return [[cells.get((row, column)) for column in range(width)] for row in range(height)]
