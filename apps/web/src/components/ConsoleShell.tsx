import type { ReactNode } from "react";
import {
  AppBar,
  Box,
  Chip,
  CircularProgress,
  CssBaseline,
  Divider,
  LinearProgress,
  List,
  ListItemButton,
  ListItemText,
  ThemeProvider,
  Toolbar,
  Typography
} from "@mui/material";
import type { WebSnapshot } from "../types";
import { sections, type SectionId } from "../ui/navigation";
import { drawerWidth, theme } from "../ui/theme";
import { short } from "../utils/format";

export function ConsoleShell({
  section,
  snapshot,
  sessionsLength,
  toolErrors,
  loading,
  overlays,
  children,
  onSectionChange
}: {
  section: SectionId;
  snapshot: WebSnapshot | null;
  sessionsLength: number;
  toolErrors: number;
  loading: boolean;
  overlays?: ReactNode;
  children: ReactNode;
  onSectionChange: (section: SectionId) => void;
}) {
  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <Box className="app-shell">
        <AppBar position="fixed" color="default" elevation={0} sx={{ zIndex: (muiTheme) => muiTheme.zIndex.drawer + 1 }}>
          <Toolbar variant="dense" sx={{ gap: 1.5 }}>
            <Typography variant="h6" sx={{ flexGrow: 1 }}>
              teamD Web Console
            </Typography>
            {loading ? <CircularProgress size={18} /> : null}
            <Chip
              label={snapshot?.status.ok ? "agentd online" : "agentd unknown"}
              color={snapshot?.status.ok ? "success" : "warning"}
              variant="outlined"
            />
            <Chip label={`sessions: ${sessionsLength}`} variant="outlined" />
            <Chip label={`tools err: ${toolErrors}`} color={toolErrors > 0 ? "warning" : "default"} variant="outlined" />
          </Toolbar>
        </AppBar>

        <Box component="nav" className="sidebar" sx={{ width: drawerWidth }}>
          <Toolbar variant="dense" />
          <Box sx={{ p: 1.25 }}>
            <List dense disablePadding>
              {sections.map((item) => (
                <ListItemButton
                  key={item.id}
                  selected={section === item.id}
                  onClick={() => onSectionChange(item.id)}
                  sx={{ borderRadius: 1.5, mb: 0.5 }}
                >
                  <ListItemText
                    primary={item.label}
                    secondary={item.description}
                    primaryTypographyProps={{ fontWeight: 700 }}
                    secondaryTypographyProps={{ fontSize: 11 }}
                  />
                </ListItemButton>
              ))}
            </List>
          </Box>
          <Divider />
          <Box sx={{ p: 1.5 }}>
            <Typography variant="caption" color="text.secondary">
              Runtime
            </Typography>
            <Typography variant="body2" className="mono" sx={{ mt: 0.5 }}>
              {snapshot?.status.version ?? "—"} · {short(snapshot?.status.commit, 10)}
            </Typography>
            <Typography variant="caption" color="text.secondary">
              {snapshot?.status.data_dir ?? "snapshot не загружен"}
            </Typography>
          </Box>
        </Box>

        <Box component="main" className="main-panel" sx={{ ml: `${drawerWidth}px` }}>
          <Toolbar variant="dense" />
          {loading && !snapshot ? <LinearProgress sx={{ mb: 2 }} /> : null}
          {children}
        </Box>
        {overlays}
      </Box>
    </ThemeProvider>
  );
}
