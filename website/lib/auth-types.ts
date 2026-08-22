export interface AuthUser {
  id: string;
  username: string;
  display_name: string;
  avatar_url: string | null;
}

export interface AuthSession {
  token: string;
  expires_at: string;
  user: AuthUser;
}
