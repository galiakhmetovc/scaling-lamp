import { createTheme } from "@mui/material";

export const drawerWidth = 276;

export const theme = createTheme({
  palette: {
    mode: "dark",
    background: {
      default: "#0e1217",
      paper: "#151a21"
    },
    primary: {
      main: "#79c7b7",
      contrastText: "#071311"
    },
    secondary: {
      main: "#9eb6ff"
    },
    warning: {
      main: "#f2c36b"
    },
    error: {
      main: "#ff7b7b"
    },
    success: {
      main: "#84d28a"
    },
    divider: "rgba(255,255,255,0.08)"
  },
  typography: {
    fontFamily: '"IBM Plex Sans", "Segoe UI", "Noto Sans", sans-serif',
    fontSize: 13,
    h5: {
      fontWeight: 700,
      letterSpacing: "-0.02em"
    },
    h6: {
      fontWeight: 700
    },
    button: {
      textTransform: "none",
      fontWeight: 700
    }
  },
  shape: {
    borderRadius: 10
  },
  components: {
    MuiButton: {
      defaultProps: {
        size: "small"
      }
    },
    MuiChip: {
      defaultProps: {
        size: "small"
      }
    },
    MuiTextField: {
      defaultProps: {
        size: "small"
      }
    },
    MuiTableCell: {
      styleOverrides: {
        root: {
          borderColor: "rgba(255,255,255,0.07)",
          padding: "8px 10px",
          verticalAlign: "top"
        },
        head: {
          color: "rgba(255,255,255,0.72)",
          fontSize: 12,
          fontWeight: 700,
          letterSpacing: "0.02em",
          textTransform: "uppercase"
        }
      }
    },
    MuiPaper: {
      styleOverrides: {
        root: {
          backgroundImage: "none"
        }
      }
    }
  }
});
