use serde::Serialize;
use std::ffi::c_void;
use windows::core::{GUID, PCWSTR};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::NetworkManagement::IpHelper::{GetAdaptersInfo, IP_ADAPTER_INFO};
use windows::Win32::NetworkManagement::WiFi::*;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiNetwork {
    pub ssid: String,
    pub signal: u32,
    pub secured: bool,
    pub has_profile: bool,
    pub connected: bool,
    pub auth: String,
    pub cipher: String,
    pub profile_name: Option<String>,
    pub connectable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WiredInfo {
    pub name: String,
    pub ipv4: String,
    pub gateway: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiStatus {
    pub adapter: bool,
    pub enabled: bool,
    pub hardware_enabled: bool,
    pub connected: bool,
    pub ssid: Option<String>,
    pub signal: Option<u32>,
    pub auth: Option<String>,
    pub state: String,
    pub message: Option<String>,
    pub interface_name: Option<String>,
    pub networks: Vec<WifiNetwork>,
    pub wired: Option<WiredInfo>,
}

struct Session {
    handle: HANDLE,
}

impl Session {
    fn open() -> Result<Self, String> {
        let mut negotiated = 0u32;
        let mut handle = HANDLE::default();
        let code = unsafe { WlanOpenHandle(2, None, &mut negotiated, &mut handle) };
        if code != 0 {
            return Err(wlan_error(code));
        }
        Ok(Self { handle })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            let _ = WlanCloseHandle(self.handle, None);
        }
    }
}

struct Interface {
    guid: GUID,
    name: String,
    state: WLAN_INTERFACE_STATE,
}

struct Connection {
    ssid: String,
    signal: u32,
    auth: String,
    state: WLAN_INTERFACE_STATE,
}

fn wide_string(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|value| *value == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn ssid_string(ssid: &DOT11_SSID) -> String {
    let length = (ssid.uSSIDLength as usize).min(ssid.ucSSID.len());
    String::from_utf8_lossy(&ssid.ucSSID[..length]).to_string()
}

fn wlan_error(code: u32) -> String {
    match code {
        5 => "O Windows negou o acesso ao controle de Wi-Fi.".to_string(),
        87 => "Os dados de Wi-Fi enviados não são válidos.".to_string(),
        1062 => "O serviço de Wi-Fi do Windows está parado.".to_string(),
        1168 => "Rede não encontrada. Atualize a lista e tente de novo.".to_string(),
        1206 => "Nenhuma rede disponível no momento.".to_string(),
        _ => format!("O Windows não conseguiu concluir a operação de Wi-Fi (código {code})."),
    }
}

fn auth_label(auth: DOT11_AUTH_ALGORITHM, secured: bool) -> String {
    if !secured {
        return "Aberta".to_string();
    }

    match auth.0 {
        1 | 2 => "WEP".to_string(),
        3 => "WPA-Enterprise".to_string(),
        4 | 5 => "WPA-Pessoal".to_string(),
        6 => "WPA2-Enterprise".to_string(),
        7 => "WPA2-Pessoal".to_string(),
        8 => "WPA3-Enterprise".to_string(),
        9 => "WPA3-Pessoal".to_string(),
        10 => "OWE".to_string(),
        11 => "WPA3-Enterprise 192".to_string(),
        _ => "Protegida".to_string(),
    }
}

fn cipher_label(cipher: DOT11_CIPHER_ALGORITHM) -> String {
    match cipher.0 {
        0 => "Nenhuma",
        1 | 5 | 257 => "WEP",
        2 => "TKIP",
        _ => "AES",
    }
    .to_string()
}

fn first_interface(session: &Session) -> Result<Option<Interface>, String> {
    let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
    let code = unsafe { WlanEnumInterfaces(session.handle, None, &mut list) };
    if code != 0 {
        return Err(wlan_error(code));
    }
    if list.is_null() {
        return Ok(None);
    }

    let interface = unsafe {
        let count = (*list).dwNumberOfItems as usize;
        let info = if count > 0 {
            Some((*list).InterfaceInfo.as_ptr().read())
        } else {
            None
        };
        WlanFreeMemory(list as *const c_void);
        info
    };

    Ok(interface.map(|interface| Interface {
        guid: interface.InterfaceGuid,
        name: wide_string(&interface.strInterfaceDescription),
        state: interface.isState,
    }))
}

fn query_radio(session: &Session, interface: &Interface) -> Option<(bool, bool)> {
    let mut size = 0u32;
    let mut data: *mut c_void = std::ptr::null_mut();
    let code = unsafe {
        WlanQueryInterface(
            session.handle,
            &interface.guid,
            wlan_intf_opcode_radio_state,
            None,
            &mut size,
            &mut data,
            None,
        )
    };
    if code != 0 || data.is_null() {
        return None;
    }

    let (software, hardware) = unsafe {
        let state = &*(data as *const WLAN_RADIO_STATE);
        let count = (state.dwNumberOfPhys as usize).clamp(1, 64);
        let mut software = false;
        let mut hardware = false;

        for index in 0..count {
            let phy = state.PhyRadioState[index];
            if phy.dot11SoftwareRadioState != dot11_radio_state_off {
                software = true;
            }
            if phy.dot11HardwareRadioState != dot11_radio_state_off {
                hardware = true;
            }
        }

        WlanFreeMemory(data);
        (software, hardware)
    };

    Some((software, hardware))
}

fn query_connection(session: &Session, interface: &Interface) -> Option<Connection> {
    let mut size = 0u32;
    let mut data: *mut c_void = std::ptr::null_mut();
    let code = unsafe {
        WlanQueryInterface(
            session.handle,
            &interface.guid,
            wlan_intf_opcode_current_connection,
            None,
            &mut size,
            &mut data,
            None,
        )
    };
    if code != 0 || data.is_null() {
        return None;
    }

    let connection = unsafe {
        let attributes = &*(data as *const WLAN_CONNECTION_ATTRIBUTES);
        let association = &attributes.wlanAssociationAttributes;
        let security = &attributes.wlanSecurityAttributes;
        let result = Connection {
            ssid: ssid_string(&association.dot11Ssid),
            signal: association.wlanSignalQuality.min(100),
            auth: auth_label(
                security.dot11AuthAlgorithm,
                security.bSecurityEnabled.as_bool(),
            ),
            state: attributes.isState,
        };
        WlanFreeMemory(data);
        result
    };

    Some(connection)
}

fn available_networks(session: &Session, interface: &Interface) -> Vec<WifiNetwork> {
    let mut list: *mut WLAN_AVAILABLE_NETWORK_LIST = std::ptr::null_mut();
    let code = unsafe {
        WlanGetAvailableNetworkList(
            session.handle,
            &interface.guid,
            0,
            None,
            &mut list,
        )
    };
    if code != 0 || list.is_null() {
        return Vec::new();
    }

    let raw = unsafe {
        let count = (*list).dwNumberOfItems as usize;
        std::slice::from_raw_parts((*list).Network.as_ptr(), count)
            .iter()
            .copied()
            .collect::<Vec<WLAN_AVAILABLE_NETWORK>>()
    };
    unsafe {
        WlanFreeMemory(list as *const c_void);
    }

    let mut networks: Vec<WifiNetwork> = Vec::new();

    for network in raw {
        let ssid = ssid_string(&network.dot11Ssid);
        if ssid.is_empty() {
            continue;
        }

        let connected = (network.dwFlags & WLAN_AVAILABLE_NETWORK_CONNECTED) != 0;
        let has_profile = (network.dwFlags & WLAN_AVAILABLE_NETWORK_HAS_PROFILE) != 0;
        let secured = network.bSecurityEnabled.as_bool();
        let profile_name = wide_string(&network.strProfileName);
        let candidate = WifiNetwork {
            ssid: ssid.clone(),
            signal: network.wlanSignalQuality.min(100),
            secured,
            has_profile,
            connected,
            auth: auth_label(
                network.dot11DefaultAuthAlgorithm,
                secured,
            ),
            cipher: cipher_label(network.dot11DefaultCipherAlgorithm),
            profile_name: if profile_name.is_empty() {
                None
            } else {
                Some(profile_name)
            },
            connectable: network.bNetworkConnectable.as_bool(),
        };

        if let Some(existing) = networks
            .iter_mut()
            .find(|item| item.ssid.eq_ignore_ascii_case(&ssid))
        {
            if candidate.connected
                || (!existing.connected && candidate.signal > existing.signal)
            {
                *existing = candidate;
            }
        } else {
            networks.push(candidate);
        }
    }

    networks.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then_with(|| b.signal.cmp(&a.signal))
            .then_with(|| a.ssid.to_lowercase().cmp(&b.ssid.to_lowercase()))
    });

    networks
}

fn connect_by_profile(session: &Session, interface: &Interface, profile: &str) -> Result<(), String> {
    let profile = to_wide(profile);
    let parameters = WLAN_CONNECTION_PARAMETERS {
        wlanConnectionMode: wlan_connection_mode_profile,
        strProfile: PCWSTR(profile.as_ptr()),
        pDot11Ssid: std::ptr::null_mut(),
        pDesiredBssidList: std::ptr::null_mut(),
        dot11BssType: dot11_BSS_type_any,
        dwFlags: 0,
    };

    let code = unsafe { WlanConnect(session.handle, &interface.guid, &parameters, None) };
    if code != 0 {
        return Err(wlan_error(code));
    }
    Ok(())
}

fn connect_open(session: &Session, interface: &Interface, ssid: &str) -> Result<(), String> {
    let bytes = ssid.as_bytes();
    let mut dot11_ssid = DOT11_SSID::default();
    let length = bytes.len().min(dot11_ssid.ucSSID.len());
    dot11_ssid.uSSIDLength = length as u32;
    dot11_ssid.ucSSID[..length].copy_from_slice(&bytes[..length]);

    let parameters = WLAN_CONNECTION_PARAMETERS {
        wlanConnectionMode: wlan_connection_mode_discovery_unsecure,
        strProfile: PCWSTR::null(),
        pDot11Ssid: &mut dot11_ssid,
        pDesiredBssidList: std::ptr::null_mut(),
        dot11BssType: dot11_BSS_type_any,
        dwFlags: 0,
    };

    let code = unsafe { WlanConnect(session.handle, &interface.guid, &parameters, None) };
    if code != 0 {
        return Err(wlan_error(code));
    }
    Ok(())
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn profile_xml(ssid: &str, password: &str, auth: &str, encryption: &str) -> String {
    let ssid = xml_escape(ssid);
    let password = xml_escape(password);

    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
  <name>{ssid}</name>
  <SSIDConfig>
    <SSID>
      <name>{ssid}</name>
    </SSID>
  </SSIDConfig>
  <connectionType>ESS</connectionType>
  <connectionMode>auto</connectionMode>
  <MSM>
    <security>
      <authEncryption>
        <authentication>{auth}</authentication>
        <encryption>{encryption}</encryption>
        <useOneX>false</useOneX>
      </authEncryption>
      <sharedKey>
        <keyType>passPhrase</keyType>
        <protected>false</protected>
        <keyMaterial>{password}</keyMaterial>
      </sharedKey>
    </security>
  </MSM>
</WLANProfile>"#
    )
}

fn save_profile(
    session: &Session,
    interface: &Interface,
    ssid: &str,
    password: &str,
    auth: &str,
    encryption: &str,
) -> Result<(), String> {
    let xml = profile_xml(ssid, password, auth, encryption);
    let xml = to_wide(&xml);
    let mut reason = 0u32;
    let code = unsafe {
        WlanSetProfile(
            session.handle,
            &interface.guid,
            0,
            PCWSTR(xml.as_ptr()),
            PCWSTR::null(),
            true,
            None,
            &mut reason,
        )
    };

    if code != 0 {
        let reason = if reason != 0 {
            format!(" (motivo {reason})")
        } else {
            String::new()
        };
        return Err(format!("{}{reason}", wlan_error(code)));
    }

    Ok(())
}

/// Quick status: when `include_networks` is false the (slower) scan of
/// available networks is skipped. The dock uses that for its icon poll.
pub fn status(include_networks: bool) -> WifiStatus {
    let wired = wired_status();
    let session = match Session::open() {
        Ok(session) => session,
        Err(message) => {
            let message = if wired.is_some() {
                "Nenhum adaptador Wi-Fi encontrado. Este computador está usando Ethernet."
                    .to_string()
            } else {
                message
            };
            return WifiStatus::unavailable(message, wired);
        }
    };

    let interface = match first_interface(&session) {
        Ok(Some(interface)) => interface,
        Ok(None) => {
            return WifiStatus::unavailable(
                "Nenhum adaptador Wi-Fi encontrado.",
                wired,
            )
        }
        Err(message) => return WifiStatus::unavailable(message, wired),
    };

    let (software_radio, hardware_radio) =
        query_radio(&session, &interface).unwrap_or((true, true));
    let enabled = software_radio && hardware_radio;

    let connection = query_connection(&session, &interface);
    let connected = connection
        .as_ref()
        .map(|connection| connection.state == wlan_interface_state_connected)
        .unwrap_or(false);
    let connecting = connection
        .as_ref()
        .map(|connection| {
            connection.state == wlan_interface_state_associating
                || connection.state == wlan_interface_state_authenticating
        })
        .unwrap_or(false);

    let networks = if enabled && include_networks {
        available_networks(&session, &interface)
    } else {
        Vec::new()
    };

    let state = if !enabled {
        "off"
    } else if connected {
        "connected"
    } else if connecting {
        "connecting"
    } else if interface.state == wlan_interface_state_not_ready {
        "starting"
    } else {
        "disconnected"
    };

    let message = match state {
        "starting" => Some("O adaptador Wi-Fi está iniciando.".to_string()),
        "off" if !hardware_radio => {
            Some("O Wi-Fi está desligado pelo botão do equipamento.".to_string())
        }
        _ => None,
    };

    WifiStatus {
        adapter: true,
        enabled,
        hardware_enabled: hardware_radio,
        connected,
        ssid: connection
            .as_ref()
            .filter(|connection| connection.state == wlan_interface_state_connected)
            .map(|connection| connection.ssid.clone())
            .filter(|ssid| !ssid.is_empty()),
        signal: connection
            .as_ref()
            .filter(|connection| connection.state == wlan_interface_state_connected)
            .map(|connection| connection.signal),
        auth: connection
            .as_ref()
            .filter(|connection| connection.state == wlan_interface_state_connected)
            .map(|connection| connection.auth.clone()),
        state: state.to_string(),
        message,
        interface_name: Some(interface.name),
        networks,
        wired,
    }
}

impl WifiStatus {
    fn unavailable(message: impl Into<String>, wired: Option<WiredInfo>) -> Self {
        Self {
            adapter: false,
            enabled: false,
            hardware_enabled: false,
            connected: false,
            ssid: None,
            signal: None,
            auth: None,
            state: "no-adapter".to_string(),
            message: Some(message.into()),
            interface_name: None,
            networks: Vec::new(),
            wired,
        }
    }
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let session = Session::open()?;
    let interface = first_interface(&session)?
        .ok_or_else(|| "Nenhum adaptador Wi-Fi encontrado.".to_string())?;

    let mut size = 0u32;
    let mut data: *mut c_void = std::ptr::null_mut();
    let code = unsafe {
        WlanQueryInterface(
            session.handle,
            &interface.guid,
            wlan_intf_opcode_radio_state,
            None,
            &mut size,
            &mut data,
            None,
        )
    };

    let mut phys: Vec<(u32, DOT11_RADIO_STATE)> = Vec::new();
    if code == 0 && !data.is_null() {
        unsafe {
            let current = &*(data as *const WLAN_RADIO_STATE);
            let count = (current.dwNumberOfPhys as usize).clamp(1, 64);
            for index in 0..count {
                let phy = current.PhyRadioState[index];
                phys.push((phy.dwPhyIndex, phy.dot11HardwareRadioState));
            }
            WlanFreeMemory(data);
        }
    }
    if phys.is_empty() {
        phys.push((0, dot11_radio_state_on));
    }

    let mut radio = WLAN_RADIO_STATE::default();
    radio.dwNumberOfPhys = phys.len() as u32;
    for (index, (phy_index, hardware)) in phys.iter().enumerate() {
        radio.PhyRadioState[index] = WLAN_PHY_RADIO_STATE {
            dwPhyIndex: *phy_index,
            dot11SoftwareRadioState: if enabled {
                dot11_radio_state_on
            } else {
                dot11_radio_state_off
            },
            dot11HardwareRadioState: *hardware,
        };
    }

    let data_size = 4 + phys.len() * std::mem::size_of::<WLAN_PHY_RADIO_STATE>();
    let code = unsafe {
        WlanSetInterface(
            session.handle,
            &interface.guid,
            wlan_intf_opcode_radio_state,
            data_size as u32,
            &radio as *const _ as *const c_void,
            None,
        )
    };

    if code != 0 {
        return Err(wlan_error(code));
    }
    Ok(())
}

pub fn disconnect() -> Result<(), String> {
    let session = Session::open()?;
    let interface = first_interface(&session)?
        .ok_or_else(|| "Nenhum adaptador Wi-Fi encontrado.".to_string())?;

    let code = unsafe { WlanDisconnect(session.handle, &interface.guid, None) };
    if code != 0 && code != 1168 {
        return Err(wlan_error(code));
    }
    Ok(())
}

pub fn forget(profile: &str) -> Result<(), String> {
    let session = Session::open()?;
    let interface = first_interface(&session)?
        .ok_or_else(|| "Nenhum adaptador Wi-Fi encontrado.".to_string())?;

    if profile.trim().is_empty() {
        return Err("Perfil de rede vazio.".to_string());
    }

    let profile = to_wide(profile);
    let code = unsafe {
        WlanDeleteProfile(
            session.handle,
            &interface.guid,
            PCWSTR(profile.as_ptr()),
            None,
        )
    };

    if code != 0 && code != 1168 {
        return Err(wlan_error(code));
    }
    Ok(())
}

pub fn connect(ssid: &str, password: Option<&str>) -> Result<(), String> {
    let session = Session::open()?;
    let interface = first_interface(&session)?
        .ok_or_else(|| "Nenhum adaptador Wi-Fi encontrado.".to_string())?;

    let (software_radio, hardware_radio) =
        query_radio(&session, &interface).unwrap_or((true, true));
    if !software_radio || !hardware_radio {
        return Err("Ative o Wi-Fi antes de conectar.".to_string());
    }

    let network = available_networks(&session, &interface)
        .into_iter()
        .find(|network| network.ssid.eq_ignore_ascii_case(ssid))
        .ok_or_else(|| "Rede não encontrada. Atualize a lista e tente de novo.".to_string())?;

    if !network.secured {
        return connect_open(&session, &interface, ssid);
    }

    if let Some(password) = password {
        let auth = match network.auth.as_str() {
            "WPA3-Pessoal" => "WPA3SAE",
            "WPA2-Pessoal" => "WPA2PSK",
            "WPA-Pessoal" => "WPAPSK",
            _ => {
                return Err(
                    "Esta rede não aceita senha simples. Redes corporativas e WEP precisam ser configuradas pelo Windows."
                        .to_string(),
                )
            }
        };

        if password.len() < 8 || password.len() > 63 {
            return Err("A senha deve ter entre 8 e 63 caracteres.".to_string());
        }

        let encryption = if network.cipher == "TKIP" {
            "TKIP"
        } else {
            "AES"
        };

        save_profile(
            &session,
            &interface,
            ssid,
            password,
            auth,
            encryption,
        )?;

        return connect_by_profile(&session, &interface, ssid);
    }

    if !network.has_profile {
        return Err("Informe a senha desta rede.".to_string());
    }

    let profile = network
        .profile_name
        .clone()
        .unwrap_or_else(|| ssid.to_string());
    connect_by_profile(&session, &interface, &profile)
}

fn wired_status() -> Option<WiredInfo> {
    let mut size = 0u32;
    let code = unsafe { GetAdaptersInfo(None, &mut size) };
    if size == 0 {
        return None;
    }
    let _ = code;

    let words = (size as usize + std::mem::size_of::<usize>() - 1)
        / std::mem::size_of::<usize>();
    let mut buffer: Vec<usize> = vec![0; words];
    let code = unsafe {
        GetAdaptersInfo(
            Some(buffer.as_mut_ptr() as *mut IP_ADAPTER_INFO),
            &mut size,
        )
    };
    if code != 0 {
        return None;
    }

    let mut current = buffer.as_ptr() as *const IP_ADAPTER_INFO;
    while !current.is_null() {
        let adapter = unsafe { &*current };
        let ip = c_string(&adapter.IpAddressList.IpAddress.String);
        let gateway = c_string(&adapter.GatewayList.IpAddress.String);
        let is_ethernet = adapter.Type == 6;

        if is_ethernet && !ip.is_empty() && ip != "0.0.0.0" && !gateway.is_empty() {
            return Some(WiredInfo {
                name: c_string(&adapter.Description),
                ipv4: ip,
                gateway,
            });
        }

        current = adapter.Next;
    }

    None
}

fn c_string(buffer: &[i8]) -> String {
    let bytes: &[u8] =
        unsafe { std::slice::from_raw_parts(buffer.as_ptr() as *const u8, buffer.len()) };
    let end = bytes.iter().position(|value| *value == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}
