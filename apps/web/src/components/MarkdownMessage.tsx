import { Box } from "@mui/material";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

export function MarkdownMessage({ content }: { content: string }) {
  return (
    <Box className="markdown-message">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: ({ href, children }) => (
            <a href={href} target="_blank" rel="noreferrer">
              {children}
            </a>
          ),
          code: ({ children, className }) => {
            const value = String(children).replace(/\n$/, "");
            const isInline = !className && !value.includes("\n");
            if (isInline) {
              return <code className="markdown-inline-code">{children}</code>;
            }
            return <code className={className}>{children}</code>;
          },
          pre: ({ children }) => <pre className="markdown-code-block">{children}</pre>,
          table: ({ children }) => (
            <Box className="markdown-table-wrap">
              <table>{children}</table>
            </Box>
          )
        }}
      >
        {content}
      </ReactMarkdown>
    </Box>
  );
}
