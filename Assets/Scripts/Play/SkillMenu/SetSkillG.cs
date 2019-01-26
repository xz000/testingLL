using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillG : MonoBehaviour
{
    //public CatcherKeys ck;
    public Toggle G1;
    public Image IconG;
    GameObject Soldier;

    public void SetG()
    {
        AllGOff();
        IconG.GetComponent<CooldownImage>().enabled = true;
        if (G1.isOn)
        {
            IconG.GetComponent<CooldownImage>().Fif = GetComponent<SkillsLink>().mySoldier.GetComponent<TestSkill01>().CalcFA;
            GetComponent<SkillsLink>().KeyGSkill = SkillCode.TestSkill01;
            return;
        }
    }

    public void AllGOff()
    {
        IconG.GetComponent<CooldownImage>().enabled = false;
        GetComponent<SkillsLink>().KeyGSkill = null;
    }
}
