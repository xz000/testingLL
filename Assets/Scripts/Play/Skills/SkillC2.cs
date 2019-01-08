using System.Collections;
using System.Collections.Generic;
using UnityEngine;
///using Photon;

public class SkillC2 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public GameObject MyShieldObj;
    public GameObject MyShield;
    private float currentcooldown;
    public float cooldowntime = 5;
    public bool skillavaliable;
    public float maxtime = 2;

    // Use this for initialization
    void Start ()
    {
        currentcooldown = cooldowntime;
    }
	
	void GoSkillC2()
    {
        if (skillavaliable)
        {
            GetComponent<DoSkill>().singing = 0;
            Skill();
        }
    }

    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
            //MyImageScript.IconFillAmount = currentcooldown / cooldowntime;
        }
    }

    void Skill()
    {
        GetComponent<DoSkill>().BeforeSkill();
        currentcooldown = 0;
        skillavaliable = false;
        MyShieldObj.GetComponent<ShieldScript>().sender = gameObject;
        MyShield = Instantiate(MyShieldObj, gameObject.transform.position, Quaternion.identity);
        //MyShield.GetComponent<ShieldScript>().sender = gameObject;
        MyShield.GetComponent<ShieldScript>().maxtime = maxtime;
        MyShield.layer = 2;
        gameObject.GetComponent<DoSkill>().DoClearJob();
    }

    void SkillC2SetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }
}
