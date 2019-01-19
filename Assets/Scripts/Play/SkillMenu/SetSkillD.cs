using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillD : MonoBehaviour
{
    public Toggle D1;
    public Toggle D2;
    public Toggle D3;
    public Toggle D4;
    //public Toggle D5;
    public Image IconD;
    public SkillsLink sls;
    GameObject Soldier;

    public void SetD()
    {
        AllDOff();
        IconD.GetComponent<CooldownImage>().enabled = true;
        if (D1.isOn)
        {
            IconD.GetComponent<CooldownImage>().Fif = sls.mySoldier.GetComponent<TestSkillLightning>().CalcFA;
            sls.KeyDSkill = SkillCode.TestSkillLightning;
            return;
        }
        if (D2.isOn)
        {
            IconD.GetComponent<CooldownImage>().Fif = sls.mySoldier.GetComponent<SkillD2>().CalcFA;
            sls.KeyDSkill = SkillCode.SkillD2;
            return;
        }
        /*if (D3.isOn)
        {
            Soldier.GetComponent<SkillD3>().MyImageScript = IconD.GetComponent<CooldownImage>();
            Soldier.GetComponent<SkillD3>().enabled = true;
            return;
        }
        if (D4.isOn)
        {
            Soldier.GetComponent<SkillD4>().MyImageScript = IconD.GetComponent<CooldownImage>();
            Soldier.GetComponent<SkillD4>().enabled = true;
            return;
        }*/
    }

    public void AllDOff()
    {
        IconD.GetComponent<CooldownImage>().enabled = false;
        sls.KeyDSkill = null;
    }
}
