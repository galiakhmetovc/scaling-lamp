import { JsonBlock, SectionHeader } from "../../components/common";
import type { WebSnapshot } from "../../types";

export function SettingsScreen({ snapshot }: { snapshot: WebSnapshot | null }) {
  return (
    <>
      <SectionHeader title="Настройки" subtitle="Пока read-only. Запись настроек должна идти через отдельные agentd endpoints." />
      <JsonBlock
        value={{
          agentd_proxy: "/api/agentd/v1/*",
          canonical_chat_path: "/v1/chat/turn",
          snapshot: snapshot ?? null
        }}
      />
    </>
  );
}
