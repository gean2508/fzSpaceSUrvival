import type { ReactNode } from "react";
import {
  ScrollView,
  StyleSheet,
  Text,
  TouchableOpacity,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { COLORS } from "../../theme/colors";

interface MenuLayoutProps {
  children: ReactNode;
  title?: string;
  onBack?: () => void;
}

export function MenuLayout({ children, title, onBack }: MenuLayoutProps) {
  return (
    <SafeAreaView style={styles.root}>
      <View style={styles.skyTop} />
      <View style={styles.hills} />
      <ScrollView
        contentContainerStyle={styles.content}
        keyboardShouldPersistTaps="handled"
      >
        {onBack && (
          <TouchableOpacity style={styles.backBtn} onPress={onBack}>
            <Text style={styles.backText}>◀ Atrás</Text>
          </TouchableOpacity>
        )}
        {title && <Text style={styles.title}>{title}</Text>}
        {children}
      </ScrollView>
    </SafeAreaView>
  );
}

export function PrimaryButton({
  children,
  onPress,
  style,
}: {
  children: ReactNode;
  onPress?: () => void;
  style?: object;
}) {
  return (
    <TouchableOpacity style={[styles.primary, style]} onPress={onPress} activeOpacity={0.85}>
      <Text style={styles.primaryText}>{children}</Text>
    </TouchableOpacity>
  );
}

const styles = StyleSheet.create({
  root: {
    flex: 1,
    backgroundColor: COLORS.skyTop,
  },
  skyTop: {
    ...StyleSheet.absoluteFill,
    backgroundColor: COLORS.skyTop,
  },
  hills: {
    position: "absolute",
    left: 0,
    right: 0,
    bottom: 0,
    height: "34%",
    backgroundColor: COLORS.meadowDeep,
    borderTopLeftRadius: 120,
    borderTopRightRadius: 120,
  },
  content: {
    flexGrow: 1,
    alignItems: "center",
    justifyContent: "center",
    padding: 20,
    paddingTop: 48,
  },
  backBtn: {
    position: "absolute",
    left: 16,
    top: 8,
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderRadius: 8,
    backgroundColor: "rgba(0,0,0,0.12)",
  },
  backText: {
    fontWeight: "600",
    color: COLORS.night,
    fontSize: 14,
  },
  title: {
    fontSize: 28,
    fontWeight: "800",
    color: COLORS.night,
    marginBottom: 20,
    textAlign: "center",
  },
  primary: {
    borderRadius: 16,
    backgroundColor: COLORS.sunset,
    paddingHorizontal: 32,
    paddingVertical: 16,
    shadowColor: COLORS.bark,
    shadowOffset: { width: 0, height: 6 },
    shadowOpacity: 0.35,
    shadowRadius: 0,
    elevation: 4,
  },
  primaryText: {
    color: COLORS.white,
    fontSize: 18,
    fontWeight: "800",
    textAlign: "center",
  },
});
