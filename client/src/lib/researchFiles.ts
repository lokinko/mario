const MAX_BYTES = 5 * 1024 * 1024;

export function validateResearchFiles(files: File[]): string {
  if (files.length > 5) return "每条资料最多附带 5 个文件";
  if (files.reduce((total, file) => total + file.size, 0) > MAX_BYTES)
    return "附件总大小不能超过 5 MB";
  if (
    files.some(
      (file) => file.name.length > 240 || /[\x00-\x1f/\\]/.test(file.name),
    )
  )
    return "附件文件名无效";
  return "";
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export async function encodeResearchFiles(files: File[]) {
  const message = validateResearchFiles(files);
  if (message) throw new Error(message);
  return Promise.all(
    files.map(
      (file) =>
        new Promise<{ name: string; dataBase64: string }>((resolve, reject) => {
          const reader = new FileReader();
          reader.onerror = () =>
            reject(new Error(`无法读取附件：${file.name}`));
          reader.onabort = () =>
            reject(new Error(`附件读取已取消：${file.name}`));
          reader.onload = () => {
            const result = String(reader.result);
            const separator = result.indexOf(",");
            if (separator < 0) reject(new Error(`无法读取附件：${file.name}`));
            else
              resolve({
                name: file.name,
                dataBase64: result.slice(separator + 1),
              });
          };
          reader.readAsDataURL(file);
        }),
    ),
  );
}
