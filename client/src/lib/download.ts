export async function saveDownload(
  name: string,
  data: Uint8Array,
  mimeType = "application/octet-stream",
): Promise<void> {
  const url = URL.createObjectURL(new Blob([data], { type: mimeType }));
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  document.body.appendChild(link);
  try {
    link.click();
  } finally {
    link.remove();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
}

export function saveTextDownload(name: string, text: string, mimeType: string) {
  return saveDownload(name, new TextEncoder().encode(text), mimeType);
}

export function saveAttachmentDownload(file: {
  name: string;
  dataBase64: string;
}) {
  return saveDownload(
    file.name,
    Uint8Array.from(atob(file.dataBase64), (c) => c.charCodeAt(0)),
  );
}
