using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

[RequireComponent(typeof(GetKeyScript))]
[RequireComponent(typeof(Toggle))]
public class SetKeyScript : MonoBehaviour
{
    public KeySetToggleScript keySetToggleScript;
    public void SetPref(string s)
    {
        PlayerPrefs.SetString(GetComponent<GetKeyScript>().MyPrefName, s);
        GetComponent<GetKeyScript>().SetText();
    }

    public void SelfEnable(bool a)
    {
        enabled = a;
    }

    private void OnGUI()
    {
        Event e = Event.current;
        if (e.isKey)
        {
            enabled = false;
            GetComponent<Toggle>().isOn = false;
            keySetToggleScript.FindExistAndHandle(e.keyCode.ToString(), GetComponent<GetKeyScript>().GetName());
            SetPref(e.keyCode.ToString());
        }
    }
}
